//! Host side (never on the guest): a `Source` over vectors for the tests, the bids file, and the
//! real hash through the `commit/` helper guest on the zkVM's emulator.

use crate::{Bid, Source, MAX_BIDS};
use std::cell::Cell;
use std::path::PathBuf;
use std::process::Command;

/// A [`Source`] over vectors with a stand-in hash. Reading past the end of either vector is what
/// the guest cannot do — on the zkVM it is an unsatisfiable request, so no proof — and is
/// recorded in `oob`.
pub struct Mock {
    pub public: Vec<u32>,
    pub input: Vec<u32>,
    pub oob: Cell<bool>,
}

impl Mock {
    pub fn new(public: &[u32], input: &[u32]) -> Mock {
        Mock { public: public.to_vec(), input: input.to_vec(), oob: Cell::new(false) }
    }

    /// A call over `bids` in `mode`, with two fixed blind words.
    pub fn call(mode: u32, bids: &[Bid]) -> Mock {
        let mut input = vec![0xb11d_0000, 0xb11d_0001];
        input.extend(call_words(bids));
        Mock::new(&[mode], &input)
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

/// A stand-in hash for the tests: deterministic, sensitive to every word and to the length,
/// **not** Poseidon2. The real hash is exercised by `run.sh` on the emulator.
pub fn test_hash(words: &[u32]) -> [u32; 8] {
    let mut h = [0u32; 8];
    let mut x: u64 = 0xcbf2_9ce4_8422_2325 ^ (words.len() as u64);
    for (i, out) in h.iter_mut().enumerate() {
        for w in words {
            x ^= u64::from(*w) ^ ((i as u64) << 40);
            x = x.wrapping_mul(0x100_0000_01b3).rotate_left(17);
        }
        *out = (x >> 16) as u32;
    }
    h
}

impl Source for Mock {
    fn public(&self, i: u32) -> u32 {
        self.get(&self.public, i)
    }
    fn input(&self, i: u32) -> u32 {
        self.get(&self.input, i)
    }
    fn hash6(&self, msg: [u32; 6]) -> [u32; 8] {
        test_hash(&msg)
    }
    fn hash17(&self, msg: [u32; 17]) -> [u32; 8] {
        test_hash(&msg)
    }
}

/// The private input words after the two blind words: `[n, tag bid_lo bid_hi salt0 salt1 × n]`.
pub fn call_words(bids: &[Bid]) -> Vec<u32> {
    let mut w = vec![bids.len() as u32];
    for b in bids {
        w.extend_from_slice(&b.words());
    }
    w
}

/// A bids file: one bid per line, `tag bid salt` in decimal (the salt a u64 the bidder chose),
/// `#` comments and blank lines skipped.
pub fn parse_bids(text: &str) -> Result<Vec<Bid>, String> {
    let mut bids = vec![];
    for (ln, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 3 {
            return Err(format!("line {}: expected `tag bid salt`, got `{line}`", ln + 1));
        }
        let tag = f[0].parse::<u32>().map_err(|_| format!("line {}: tag `{}` is not a u32", ln + 1, f[0]))?;
        let amount = f[1].parse::<u64>().map_err(|_| format!("line {}: bid `{}` is not a u64", ln + 1, f[1]))?;
        let salt = f[2].parse::<u64>().map_err(|_| format!("line {}: salt `{}` is not a u64", ln + 1, f[2]))?;
        bids.push(Bid { tag, amount, salt: [salt as u32, (salt >> 32) as u32] });
    }
    Ok(bids)
}

/// Eight words as 64 hex, each word little-endian: how the commitment list is published.
pub fn hex8(w: &[u32; 8]) -> String {
    w.iter().flat_map(|x| x.to_le_bytes()).map(|b| format!("{b:02x}")).collect()
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

/// A published commitment list: one 64-hex commitment per line, `#` comments and blank lines
/// skipped.
pub fn parse_commitments(text: &str) -> Result<Vec<[u32; 8]>, String> {
    let mut cs = vec![];
    for (ln, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        // Allow a leading index ("1  <hex>"), as `auction show` prints the list.
        let hex = line.split_whitespace().last().unwrap_or("");
        cs.push(parse_hex8(hex).map_err(|e| format!("line {}: {e}", ln + 1))?);
    }
    if cs.is_empty() || cs.len() > MAX_BIDS {
        return Err(format!("{} commitments; the program folds 1 to {MAX_BIDS}", cs.len()));
    }
    Ok(cs)
}

/// The real Poseidon2, computed by the `commit/` helper guest on the zkVM's emulator: the
/// guest's own hash code, not a host reimplementation of it. Needs `$CIRCUITS` (with `rand-guest`
/// built there) and the helper's image, `$COMMIT_IMAGE` or `commit/image.bin` beside `core/`.
pub struct Emu {
    rg: PathBuf,
    image: PathBuf,
}

impl Emu {
    pub fn new() -> Result<Emu, String> {
        let circuits = std::env::var("CIRCUITS").map_err(|_| "CIRCUITS is not set".to_string())?;
        let rg = PathBuf::from(&circuits).join("rand-guest/target/release/rand-guest");
        if !rg.is_file() {
            return Err(format!("no rand-guest at {} (build it in the circuits checkout)", rg.display()));
        }
        let image = match std::env::var("COMMIT_IMAGE") {
            Ok(p) => PathBuf::from(p),
            Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commit/image.bin"),
        };
        if !image.is_file() {
            return Err(format!("no commit image at {} (run ./build.sh)", image.display()));
        }
        Ok(Emu { rg, image })
    }

    /// The eight output words of one run of the helper over `input`.
    fn run(&self, input: &[u32]) -> Result<[u32; 8], String> {
        let mut cmd = Command::new(&self.rg);
        cmd.arg("run").arg(&self.image).arg("--input");
        for w in input {
            cmd.arg(w.to_string());
        }
        let out = cmd.output().map_err(|e| format!("running {}: {e}", self.rg.display()))?;
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
            return Err(format!("commit/image.bin did not print eight words:\n{text}{}", String::from_utf8_lossy(&out.stderr)));
        }
        Ok(d)
    }

    /// `POSEIDON2([TAG_BID, tag, bid_lo, bid_hi, salt0, salt1])`.
    pub fn commitment(&self, b: &Bid) -> Result<[u32; 8], String> {
        let mut input = vec![0u32];
        input.extend_from_slice(&b.words());
        self.run(&input)
    }

    /// The fold of `cs`, in order, from eight zero words.
    pub fn fold(&self, cs: &[[u32; 8]]) -> Result<[u32; 8], String> {
        let mut input = vec![1u32, cs.len() as u32];
        for c in cs {
            input.extend_from_slice(c);
        }
        self.run(&input)
    }
}

/// Print `error: …` and exit 1.
pub fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

pub fn join(w: &[u32]) -> String {
    w.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}
