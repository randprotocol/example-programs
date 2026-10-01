//! Host side (never on the guest): build a transition, run a program's rules over it, and talk
//! to the scripts.
//!
//! - [`Transition`] is one invoke: its context words (what the program is proved over) and its
//!   `t.json` (what `rand program invoke --transition` takes).
//! - [`Mock`] is a [`Source`] over vectors, so each example's tests and `plan` tool run the very
//!   rules the guest runs.
//! - [`digest_of`] computes `POSEIDON2([tag, s0..s7])` by running `secret-hash` on the zkVM's
//!   emulator — the guest's own hash, not a host reimplementation of it.
//! - [`Args`], [`hex8`], [`parse_hex8`], [`max_satisfying`]: the `plan` tools' plumbing.

use crate::ctx::{Source, CONTEXT_VERSION, INFLOW_BURN, INFLOW_DEPOSIT, INFLOW_NONE};
use std::cell::Cell as StdCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

/// The eight call-binding words. On chain they bind the proof to its transaction; off chain the
/// emulator runs over zeros.
pub const BINDING: [u32; 8] = [0; 8];

/// What the bundle's `burn_a` is under this invoke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inflow {
    None,
    Deposit,
    Burn,
}

/// One payout or mint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Out {
    pub asset: u32,
    pub amount: u64,
    /// `rand1…`; `None` pays the invoking wallet.
    pub to: Option<String>,
}

impl Out {
    pub fn new(asset: u32, amount: u64) -> Out {
        Out { asset, amount, to: None }
    }
}

/// One cell of a transition: key and value.
pub type Kv = ([u32; 8], [u32; 8]);

/// A whole state transition, as `rand program invoke` declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub reads: Vec<Kv>,
    pub writes: Vec<Kv>,
    pub burn_r: u64,
    pub inflow: Inflow,
    pub burn_asset: u32,
    pub burn_a: u64,
    pub pays: Vec<Out>,
    pub mints: Vec<Out>,
}

impl Default for Transition {
    fn default() -> Self {
        Transition {
            reads: vec![],
            writes: vec![],
            burn_r: 0,
            inflow: Inflow::None,
            burn_asset: 0,
            burn_a: 0,
            pays: vec![],
            mints: vec![],
        }
    }
}

/// Keys compare as the chain orders them: as 32 bytes, each word little-endian.
pub fn key_bytes(k: &[u32; 8]) -> [u8; 32] {
    let mut b = [0u8; 32];
    for (i, w) in k.iter().enumerate() {
        b[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    b
}

impl Transition {
    pub fn new() -> Transition {
        Transition::default()
    }

    pub fn read(mut self, key: [u32; 8], value: [u32; 8]) -> Self {
        self.reads.push((key, value));
        self
    }

    pub fn write(mut self, key: [u32; 8], value: [u32; 8]) -> Self {
        self.writes.push((key, value));
        self
    }

    /// RAND into the vault, through the bundle's `burn_r`.
    pub fn rand_in(mut self, amount: u64) -> Self {
        self.burn_r = amount;
        self
    }

    /// A token into the vault.
    pub fn deposit(mut self, asset: u32, amount: u64) -> Self {
        self.inflow = Inflow::Deposit;
        self.burn_asset = asset;
        self.burn_a = amount;
        self
    }

    /// A token destroyed (it must be the program's own).
    pub fn burn(mut self, asset: u32, amount: u64) -> Self {
        self.inflow = Inflow::Burn;
        self.burn_asset = asset;
        self.burn_a = amount;
        self
    }

    pub fn pay(mut self, asset: u32, amount: u64) -> Self {
        self.pays.push(Out::new(asset, amount));
        self
    }

    pub fn mint(mut self, asset: u32, amount: u64) -> Self {
        self.mints.push(Out::new(asset, amount));
        self
    }

    /// Sort reads and writes into the chain's key order (strictly ascending, as it requires).
    pub fn sorted(mut self) -> Self {
        self.reads.sort_by_key(|(k, _)| key_bytes(k));
        self.writes.sort_by_key(|(k, _)| key_bytes(k));
        self
    }

    /// Where read `key` lands among the reads once sorted, and where write `key` lands.
    pub fn read_index(&self, key: &[u32; 8]) -> Option<usize> {
        self.reads.iter().position(|(k, _)| k == key)
    }

    /// The context words the program is proved over (fullnode `Transition::context`).
    pub fn context(&self) -> Vec<u32> {
        let mut c = vec![
            CONTEXT_VERSION,
            self.reads.len() as u32,
            self.writes.len() as u32,
            self.pays.len() as u32,
            self.mints.len() as u32,
            self.burn_r as u32,
            (self.burn_r >> 32) as u32,
            match self.inflow {
                Inflow::None => INFLOW_NONE,
                Inflow::Deposit => INFLOW_DEPOSIT,
                Inflow::Burn => INFLOW_BURN,
            },
            self.burn_asset,
            self.burn_a as u32,
            (self.burn_a >> 32) as u32,
        ];
        for (k, v) in self.reads.iter().chain(self.writes.iter()) {
            c.extend_from_slice(k);
            c.extend_from_slice(v);
        }
        for o in self.pays.iter().chain(self.mints.iter()) {
            c.extend_from_slice(&[o.asset, o.amount as u32, (o.amount >> 32) as u32]);
        }
        c
    }

    /// `public ‖ call binding ‖ context`: the emulator's `--public` words.
    pub fn segment(&self, public: &[u32]) -> Vec<u32> {
        let mut seg = public.to_vec();
        seg.extend_from_slice(&BINDING);
        seg.extend(self.context());
        seg
    }

    /// Whether the segment fits the 128-row public table (fullnode `segment_fits`): at most 127
    /// words in all, for a program whose public input and binding fit it.
    pub fn fits(&self, public_len: usize) -> bool {
        public_len + 8 + self.context().len() <= 127
    }

    /// The transition file `rand program invoke --transition` takes.
    pub fn json(&self) -> String {
        let cells = |v: &Vec<Kv>| {
            v.iter()
                .map(|(k, val)| format!("{{ \"key\": \"{}\", \"value\": \"{}\" }}", hex8(k), hex8(val)))
                .collect::<Vec<_>>()
                .join(",\n    ")
        };
        let outs = |v: &Vec<Out>| {
            v.iter()
                .map(|o| match &o.to {
                    Some(to) => format!("{{ \"asset\": {}, \"amount\": \"{}\", \"to\": \"{}\" }}", o.asset, o.amount, to),
                    None => format!("{{ \"asset\": {}, \"amount\": \"{}\" }}", o.asset, o.amount),
                })
                .collect::<Vec<_>>()
                .join(",\n    ")
        };
        let kind = match self.inflow {
            Inflow::None => "none",
            Inflow::Deposit => "deposit",
            Inflow::Burn => "burn",
        };
        let mut s = String::from("{\n");
        let _ = writeln!(s, "  \"reads\": [\n    {}\n  ],", cells(&self.reads));
        let _ = writeln!(s, "  \"writes\": [\n    {}\n  ],", cells(&self.writes));
        let _ = writeln!(
            s,
            "  \"deposit\": {{ \"rand\": \"{}\", \"asset\": {}, \"amount\": \"{}\", \"kind\": \"{}\" }},",
            self.burn_r, self.burn_asset, self.burn_a, kind
        );
        let _ = writeln!(s, "  \"pays\": [\n    {}\n  ],", outs(&self.pays));
        let _ = writeln!(s, "  \"mints\": [\n    {}\n  ]", outs(&self.mints));
        s.push('}');
        s
    }
}

/// A [`Source`] over vectors. Reading past the end of any of them is what the guest cannot do —
/// on the zkVM it is an unsatisfiable request, so no proof — and is recorded in `oob`.
pub struct Mock {
    pub public: Vec<u32>,
    pub ctx: Vec<u32>,
    pub input: Vec<u32>,
    pub hasher: Hasher,
    pub oob: StdCell<bool>,
}

/// A hash for [`Mock`]: [`test_hash`] in unit tests, [`real_hash`] wherever the emulator will run
/// the same words.
pub type Hasher = fn([u32; 9]) -> [u32; 8];

impl Mock {
    pub fn new(public: &[u32], t: &Transition, input: &[u32], hasher: Hasher) -> Mock {
        Mock { public: public.to_vec(), ctx: t.context(), input: input.to_vec(), hasher, oob: StdCell::new(false) }
    }

    /// Over raw context words, for tests that tamper with them.
    pub fn raw(public: &[u32], ctx: &[u32], input: &[u32], hasher: Hasher) -> Mock {
        Mock { public: public.to_vec(), ctx: ctx.to_vec(), input: input.to_vec(), hasher, oob: StdCell::new(false) }
    }

    fn get(&self, v: &[u32], i: u32) -> u32 {
        match v.get(i as usize) {
            Some(w) => *w,
            None => {
                self.oob.set(true);
                0
            }
        }
    }

    /// Run `check` and say whether the guest would have a proof: it returned `Ok` and never read
    /// a word that does not exist.
    pub fn accepts<T, E>(&self, check: impl Fn(&Self) -> Result<T, E>) -> Option<T> {
        match check(self) {
            Ok(out) if !self.oob.get() => Some(out),
            _ => None,
        }
    }
}

impl Source for Mock {
    fn public(&self, i: u32) -> u32 {
        self.get(&self.public, i)
    }
    fn ctx(&self, i: u32) -> u32 {
        self.get(&self.ctx, i)
    }
    fn input(&self, i: u32) -> u32 {
        self.get(&self.input, i)
    }
    fn hash9(&self, msg: [u32; 9]) -> [u32; 8] {
        (self.hasher)(msg)
    }
}

/// A stand-in hash for unit tests: deterministic and input-sensitive, **not** Poseidon2. Tests
/// compute every expected digest with it, so the rules are exercised exactly; the real hash is
/// exercised by each example's `run.sh` on the emulator.
pub fn test_hash(msg: [u32; 9]) -> [u32; 8] {
    let mut h = [0u32; 8];
    let mut x: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, out) in h.iter_mut().enumerate() {
        for w in msg.iter() {
            x ^= *w as u64 ^ ((i as u64) << 40);
            x = x.wrapping_mul(0x100_0000_01b3).rotate_left(17);
        }
        *out = (x >> 16) as u32;
    }
    h
}

/// The real `POSEIDON2([tag, s0..s7])`, computed by `secret-hash` on the emulator. Needs
/// `$CIRCUITS` (and `rand-guest` built there) and `$SECRET_HASH`, the image `kit/build.sh`
/// makes; `scripts/env.sh` sets both.
pub fn digest_of(tag: u32, secret: &[u32; 8]) -> Result<[u32; 8], String> {
    let circuits = std::env::var("CIRCUITS").map_err(|_| "CIRCUITS is not set".to_string())?;
    let rg = PathBuf::from(&circuits).join("rand-guest/target/release/rand-guest");
    let image = std::env::var("SECRET_HASH").map_err(|_| "SECRET_HASH is not set (source scripts/env.sh)".to_string())?;
    let mut cmd = Command::new(&rg);
    cmd.arg("run").arg(&image).arg("--input").arg(tag.to_string());
    for w in secret {
        cmd.arg(w.to_string());
    }
    let out = cmd.output().map_err(|e| format!("running {}: {e}", rg.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut d = [0u32; 8];
    let mut seen = 0;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("out[") {
            if let Some((i, v)) = rest.split_once("] = ") {
                if let (Ok(i), Ok(v)) = (i.parse::<usize>(), v.trim().parse::<u32>()) {
                    if i < 8 {
                        d[i] = v;
                        seen += 1;
                    }
                }
            }
        }
    }
    if seen != 8 {
        return Err(format!("secret-hash did not print eight words:\n{text}{}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(d)
}

thread_local! {
    static MEMO: std::cell::RefCell<HashMap<[u32; 9], [u32; 8]>> = std::cell::RefCell::new(HashMap::new());
}

/// The real Poseidon2 as a [`Hasher`] (through [`digest_of`], memoised); exits on failure.
pub fn real_hash(m: [u32; 9]) -> [u32; 8] {
    if let Some(d) = MEMO.with(|c| c.borrow().get(&m).copied()) {
        return d;
    }
    let s = [m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]];
    let d = digest_of(m[0], &s).unwrap_or_else(|e| fail(&e));
    MEMO.with(|c| c.borrow_mut().insert(m, d));
    d
}

/// Eight words as 64 hex, each word little-endian: the chain's spelling of a cell key or value.
pub fn hex8(w: &[u32; 8]) -> String {
    key_bytes(w).iter().map(|b| format!("{b:02x}")).collect()
}

/// 64 hex (with or without `0x`) as eight words.
pub fn parse_hex8(s: &str) -> Result<[u32; 8], String> {
    let s = s.trim().trim_start_matches("0x");
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("not 64 hex: {s}"));
    }
    let mut w = [0u32; 8];
    for (i, out) in w.iter_mut().enumerate() {
        let mut b = [0u8; 4];
        for (j, byte) in b.iter_mut().enumerate() {
            let at = 8 * i + 2 * j;
            *byte = u8::from_str_radix(&s[at..at + 2], 16).map_err(|e| e.to_string())?;
        }
        *out = u32::from_le_bytes(b);
    }
    Ok(w)
}

/// Eight words from a file of whitespace-separated decimals (a `secret.txt`, a `lock.txt`).
pub fn read_words8(path: &str) -> Result<[u32; 8], String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let v: Vec<u32> = text.split_whitespace().map(|t| t.parse::<u32>()).collect::<Result<_, _>>().map_err(|e| format!("{path}: {e}"))?;
    v.try_into().map_err(|_| format!("{path}: not eight words"))
}

/// The largest `x` in `0..=hi` with `ok(x)`, given `ok` holds at 0 and is monotone (true, then
/// false). The wallet's side of "verify, don't compute": it searches the very inequality the
/// program checks, so the two never disagree about rounding.
pub fn max_satisfying(hi: u64, ok: impl Fn(u64) -> bool) -> u64 {
    let (mut lo, mut hi) = (0u64, hi);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// The smallest `x` in `lo..=u64::MAX >> 1` with `ok(x)`, given `ok` is monotone (false, then
/// true) and holds somewhere in range; `None` if it never does.
pub fn min_satisfying(lo: u64, ok: impl Fn(u64) -> bool) -> Option<u64> {
    let (mut lo, mut hi) = (lo, u64::MAX >> 1);
    if !ok(hi) {
        return None;
    }
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if ok(mid) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

/// `--name value` arguments after a subcommand.
pub struct Args {
    pub cmd: String,
    map: HashMap<String, String>,
}

impl Args {
    pub fn parse() -> Args {
        let mut it = std::env::args().skip(1);
        let cmd = it.next().unwrap_or_default();
        let mut map = HashMap::new();
        let rest: Vec<String> = it.collect();
        let mut i = 0;
        while i < rest.len() {
            let k = rest[i].trim_start_matches("--").to_string();
            let v = rest.get(i + 1).cloned().unwrap_or_default();
            map.insert(k, v);
            i += 2;
        }
        Args { cmd, map }
    }

    pub fn opt(&self, k: &str) -> Option<&str> {
        self.map.get(k).map(|s| s.as_str())
    }

    pub fn str(&self, k: &str) -> &str {
        self.opt(k).unwrap_or_else(|| fail(&format!("missing --{k}")))
    }

    pub fn u64(&self, k: &str) -> u64 {
        self.str(k).parse().unwrap_or_else(|_| fail(&format!("--{k}: not a number")))
    }

    pub fn u64_or(&self, k: &str, d: u64) -> u64 {
        self.opt(k).map(|_| self.u64(k)).unwrap_or(d)
    }

    pub fn u32(&self, k: &str) -> u32 {
        self.str(k).parse().unwrap_or_else(|_| fail(&format!("--{k}: not a u32")))
    }

    /// A cell value as 64 hex; absent, or given empty, it is the absent cell (eight zeros).
    pub fn cell(&self, k: &str) -> [u32; 8] {
        match self.opt(k).map(str::trim) {
            None | Some("") => [0; 8],
            Some(s) => parse_hex8(s).unwrap_or_else(|e| fail(&format!("--{k}: {e}"))),
        }
    }

    pub fn words8(&self, k: &str) -> [u32; 8] {
        read_words8(self.str(k)).unwrap_or_else(|e| fail(&e))
    }
}

/// Print `error: …` and exit 1.
pub fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

/// What a `plan` tool hands the scripts for one invoke: `t.json` and the private inputs, written
/// to `--t` and `--i` if given, and the emulator's words printed as `public: …` / `input: …`.
pub fn emit(args: &Args, public: &[u32], t: &Transition, input: &[u32]) {
    if !t.fits(public.len()) {
        fail(&format!("the context is {} words; with {} public words it does not fit the segment", t.context().len(), public.len()));
    }
    if let Some(p) = args.opt("t") {
        std::fs::write(p, t.json()).unwrap_or_else(|e| fail(&format!("{p}: {e}")));
    }
    if let Some(p) = args.opt("i") {
        let body = input.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(", ");
        std::fs::write(p, format!("[{body}]")).unwrap_or_else(|e| fail(&format!("{p}: {e}")));
    }
    println!("public: {}", join(&t.segment(public)));
    println!("input: {}", join(input));
}

pub fn join(w: &[u32]) -> String {
    w.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

/// One step of a `plan demo`: a label, the emulator's words, and whether the program should
/// accept. `run.sh` runs each on the emulator and says if the guest disagrees.
pub fn demo_step(label: &str, public: &[u32], t: &Transition, input: &[u32], accept: bool) {
    assert!(t.fits(public.len()), "{label}: the context does not fit the segment");
    println!("step: {label}");
    println!("public: {}", join(&t.segment(public)));
    println!("input: {}", join(input));
    println!("expect: {}", if accept { "accept" } else { "refuse" });
}

/// The context words of an accepted transition that can be changed and **still be accepted**:
/// each word in turn is flipped in its lowest bit, then in its highest, and the rules re-run.
/// A program that checks every word it is shown has none — this is how each example's tests
/// demand it. Returns `(word index, flipped value)` for each change that slipped through.
pub fn loose_words<T, E>(
    public: &[u32],
    t: &Transition,
    input: &[u32],
    hasher: Hasher,
    check: impl Fn(&Mock) -> Result<T, E>,
) -> Vec<(usize, u32)> {
    let ctx = t.context();
    assert!(Mock::raw(public, &ctx, input, hasher).accepts(&check).is_some(), "the untampered transition must be accepted");
    let mut loose = vec![];
    for i in 0..ctx.len() {
        for bit in [1u32, 1 << 31] {
            let mut c = ctx.clone();
            c[i] ^= bit;
            if Mock::raw(public, &c, input, hasher).accepts(&check).is_some() {
                loose.push((i, c[i]));
            }
        }
    }
    loose
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_and_key_order() {
        let k = [1, 0x0403_0201, 0, 0, 0, 0, 0, 0xffff_ffff];
        assert_eq!(parse_hex8(&hex8(&k)).unwrap(), k);
        assert!(hex8(&k).starts_with("0100000001020304"));
        let t = Transition::new().read([2, 0, 0, 0, 0, 0, 0, 0], [0; 8]).read([1, 9, 0, 0, 0, 0, 0, 0], [0; 8]).sorted();
        assert_eq!(t.reads[0].0[0], 1);
    }

    #[test]
    fn context_layout() {
        let t = Transition::new()
            .read([1; 8], [2; 8])
            .write([1; 8], [3; 8])
            .rand_in(5_000_000_000)
            .pay(7, 9)
            .mint(8, 10);
        let c = t.context();
        assert_eq!(&c[..11], &[1, 1, 1, 1, 1, 705032704, 1, 0, 0, 0, 0]);
        assert_eq!(c.len(), 11 + 32 + 6);
        assert_eq!(&c[43..], &[7, 9, 0, 8, 10, 0]);
    }

    #[test]
    fn searches() {
        assert_eq!(max_satisfying(1000, |x| x * x <= 500), 22);
        assert_eq!(max_satisfying(0, |_| true), 0);
        assert_eq!(min_satisfying(0, |x| x >= 34), Some(34));
        assert_eq!(min_satisfying(0, |x| x.saturating_mul(3) >= 100), Some(34));
    }
}
