//! Host side (never on the guest): the issuer's tree, a holder's path, and the rules run over
//! vectors so the tests and the `issuer` tool use the very code the guest runs.
//!
//! - [`Emulator`] is the real hash: it runs `hash/image.bin` on the zkVM's emulator, once per
//!   distinct message (memoised), so an issuer's root is computed by the deployed hash code and
//!   not by a host reimplementation of it.
//! - [`TestHash`] is a stand-in for unit tests: deterministic and input-sensitive, not Poseidon2.
//! - [`Tree`] builds the [`DEPTH`]-level tree over a roll of at most [`LEAVES`] credentials and
//!   hands out a holder's path words.
//! - [`Mock`] is a [`Source`] over vectors; [`Mock::accepts`] says whether the guest would have a
//!   proof.

use crate::{leaf_of, node_of, Credential, Hash, Source, DEPTH, EMPTY, LEAVES};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// The real `POSEIDON2`, by running `hash/image.bin` on the emulator. Needs `$CIRCUITS` (and
/// `rand-guest` built there); the image is `$ELIGIBILITY_HASH`, default `../hash/image.bin`
/// beside this crate.
pub struct Emulator {
    rg: PathBuf,
    image: PathBuf,
    memo: RefCell<HashMap<Vec<u32>, [u32; 8]>>,
    pub runs: Cell<usize>,
}

impl Emulator {
    pub fn new() -> Result<Emulator, String> {
        let circuits = std::env::var("CIRCUITS").map_err(|_| "CIRCUITS is not set (see scripts/env.sh)".to_string())?;
        let rg = PathBuf::from(&circuits).join("rand-guest/target/release/rand-guest");
        if !rg.is_file() {
            return Err(format!("no rand-guest at {} (run ./build.sh)", rg.display()));
        }
        let image = std::env::var("ELIGIBILITY_HASH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hash/image.bin"));
        if !image.is_file() {
            return Err(format!("no hash image at {} (run ./build.sh)", image.display()));
        }
        Ok(Emulator { rg, image, memo: RefCell::new(HashMap::new()), runs: Cell::new(0) })
    }

    fn run(&self, mode: u32, m: &[u32]) -> [u32; 8] {
        let mut key = vec![mode];
        key.extend_from_slice(m);
        if let Some(d) = self.memo.borrow().get(&key) {
            return *d;
        }
        let mut cmd = Command::new(&self.rg);
        cmd.arg("run").arg(&self.image).arg("--input");
        for w in &key {
            cmd.arg(w.to_string());
        }
        let out = cmd.output().unwrap_or_else(|e| fail(&format!("running {}: {e}", self.rg.display())));
        let text = String::from_utf8_lossy(&out.stdout);
        let d = parse_outputs(&text).unwrap_or_else(|| {
            fail(&format!("hash/image.bin did not print eight words:\n{text}{}", String::from_utf8_lossy(&out.stderr)))
        });
        self.runs.set(self.runs.get() + 1);
        self.memo.borrow_mut().insert(key, d);
        d
    }
}

impl Hash for Emulator {
    fn hash9(&self, m: [u32; 9]) -> [u32; 8] {
        self.run(0, &m)
    }
    fn hash17(&self, m: [u32; 17]) -> [u32; 8] {
        self.run(1, &m)
    }
}

/// The `out[i] = v` lines of a `rand-guest run`, as eight words.
pub fn parse_outputs(text: &str) -> Option<[u32; 8]> {
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
    (seen == 8).then_some(d)
}

/// A stand-in hash for unit tests: deterministic and input-sensitive, **not** Poseidon2. The
/// tests compute every expected digest with it, so the rules are exercised exactly; the real hash
/// is exercised by `run.sh` on the emulator.
pub struct TestHash;

fn mix(m: &[u32]) -> [u32; 8] {
    let mut h = [0u32; 8];
    let mut x: u64 = 0xcbf2_9ce4_8422_2325 ^ (m.len() as u64);
    for (i, out) in h.iter_mut().enumerate() {
        for w in m {
            x ^= *w as u64 ^ ((i as u64) << 40);
            x = x.wrapping_mul(0x100_0000_01b3).rotate_left(17);
        }
        *out = (x >> 16) as u32;
    }
    h
}

impl Hash for TestHash {
    fn hash9(&self, m: [u32; 9]) -> [u32; 8] {
        mix(&m)
    }
    fn hash17(&self, m: [u32; 17]) -> [u32; 8] {
        mix(&m)
    }
}

/// The issuer's tree: `levels[0]` the [`LEAVES`] leaves, `levels[DEPTH]` the one root.
pub struct Tree {
    pub levels: Vec<Vec<[u32; 8]>>,
}

impl Tree {
    /// Build over `roll` (at most [`LEAVES`] credentials, in slot order; the rest [`EMPTY`]).
    pub fn build<H: Hash + ?Sized>(h: &H, roll: &[Credential]) -> Result<Tree, String> {
        if roll.len() > LEAVES {
            return Err(format!("{} credentials; the tree holds {LEAVES}", roll.len()));
        }
        let mut level: Vec<[u32; 8]> = roll.iter().map(|c| leaf_of(h, c)).collect();
        level.resize(LEAVES, EMPTY);
        let mut levels = vec![level];
        for _ in 0..DEPTH {
            let below = levels.last().expect("a level");
            let above: Vec<[u32; 8]> = below.chunks(2).map(|p| node_of(h, &p[0], &p[1])).collect();
            levels.push(above);
        }
        Ok(Tree { levels })
    }

    pub fn root(&self) -> [u32; 8] {
        self.levels[DEPTH][0]
    }

    /// The path words for the credential in slot `index`: `[sib0..sib7, dir]` per level, leaf
    /// level first. `dir` is 1 when the path is the right child at that level (bit `l` of
    /// `index`), 0 when the left.
    pub fn path(&self, index: usize) -> Vec<u32> {
        let mut w = Vec::with_capacity(9 * DEPTH);
        let mut i = index;
        for level in &self.levels[..DEPTH] {
            w.extend_from_slice(&level[i ^ 1]);
            w.push((i & 1) as u32);
            i >>= 1;
        }
        w
    }
}

/// A holder's private inputs **without the two blinds**: the credential's six words, then the
/// path. `call.sh` and `run.sh` prepend the blinds.
pub fn witness(c: &Credential, tree: &Tree, index: usize) -> Vec<u32> {
    let mut w = vec![c.id[0], c.id[1], c.id[2], c.id[3], c.birth_year, c.nonce];
    w.extend(tree.path(index));
    w
}

/// A roll from text: one credential per line, `<32 hex id> <birth_year> <nonce>`, blank lines
/// and `#` comments skipped. Slot `i` is the `i`-th credential line. The id is 16 bytes; word
/// `k` is bytes `4k..4k+4` little-endian (the chain's spelling of a word8).
pub fn parse_roll(text: &str) -> Result<Vec<Credential>, String> {
    let mut roll = vec![];
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 3 {
            return Err(format!("line {}: want `<32 hex id> <birth_year> <nonce>`, got {} fields", n + 1, f.len()));
        }
        let id = parse_id(f[0]).map_err(|e| format!("line {}: {e}", n + 1))?;
        let birth_year = f[1].parse::<u32>().map_err(|_| format!("line {}: birth_year is not a u32", n + 1))?;
        let nonce = f[2].parse::<u32>().map_err(|_| format!("line {}: nonce is not a u32", n + 1))?;
        roll.push(Credential { id, birth_year, nonce });
    }
    if roll.len() > LEAVES {
        return Err(format!("{} credentials; the tree holds {LEAVES}", roll.len()));
    }
    Ok(roll)
}

/// 32 hex digits (with or without `0x`) as four words, each little-endian over its four bytes.
pub fn parse_id(s: &str) -> Result<[u32; 4], String> {
    let s = s.trim().trim_start_matches("0x");
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("not a 32-hex id: {s}"));
    }
    let mut id = [0u32; 4];
    for (k, w) in id.iter_mut().enumerate() {
        let mut b = [0u8; 4];
        for (j, byte) in b.iter_mut().enumerate() {
            let at = 8 * k + 2 * j;
            *byte = u8::from_str_radix(&s[at..at + 2], 16).map_err(|e| e.to_string())?;
        }
        *w = u32::from_le_bytes(b);
    }
    Ok(id)
}

/// A [`Source`] over vectors. Reading past the end of either is what the guest cannot do — on
/// the zkVM it is an unsatisfiable request, so no proof — and is recorded in `oob`.
pub struct Mock<H: Hash> {
    pub public: Vec<u32>,
    pub input: Vec<u32>,
    pub hash: H,
    pub oob: Cell<bool>,
}

impl<H: Hash> Mock<H> {
    pub fn new(public: &[u32], input: &[u32], hash: H) -> Mock<H> {
        Mock { public: public.to_vec(), input: input.to_vec(), hash, oob: Cell::new(false) }
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

impl<H: Hash> Hash for Mock<H> {
    fn hash9(&self, m: [u32; 9]) -> [u32; 8] {
        self.hash.hash9(m)
    }
    fn hash17(&self, m: [u32; 17]) -> [u32; 8] {
        self.hash.hash17(m)
    }
}

impl<H: Hash> Source for Mock<H> {
    fn public(&self, i: u32) -> u32 {
        self.get(&self.public, i)
    }
    fn input(&self, i: u32) -> u32 {
        self.get(&self.input, i)
    }
}

/// Words, space-separated.
pub fn join(w: &[u32]) -> String {
    w.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

/// Print `error: …` and exit 1.
pub fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}
