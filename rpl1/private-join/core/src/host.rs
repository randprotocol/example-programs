//! Host side (never on the guest): a party's list file as a block of words, the real commitment
//! by running `commit/image.bin` on the emulator, and the rules run over vectors so the tests and
//! the `join` tool use the very code the guest runs.
//!
//! - [`Party`] is one party's list file plus salt file, and its [`BLOCK`] words.
//! - [`Emulator`] is the real `POSEIDON2`: it runs `commit/image.bin` once per distinct block
//!   (memoised), so a commitment is computed by the deployed hash code, not a reimplementation.
//! - [`test_hash`] is a stand-in for unit tests: deterministic and input-sensitive, not Poseidon2.
//! - [`Mock`] is a [`Source`] over vectors; [`Mock::accepts`] says whether the guest would prove.

use crate::{Refusal, Source, BLOCK, MAX, MSG, PUBLIC_WORDS, TAG};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// A hash over the commitment's message.
pub type Hasher<'a> = &'a dyn Fn([u32; MSG]) -> [u32; 8];

/// One party: its own id, its keys in file order (the program insists on strictly ascending),
/// and its salt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Party {
    pub id: u64,
    pub keys: Vec<u64>,
    pub salt: [u32; 8],
}

impl Party {
    /// From a list file and a salt file (eight decimal words).
    pub fn from_files(list: &str, salt: &str) -> Result<Party, String> {
        let text = std::fs::read_to_string(list).map_err(|e| format!("{list}: {e}"))?;
        let (id, keys) = parse_list(&text).map_err(|e| format!("{list}: {e}"))?;
        Ok(Party { id, keys, salt: read_salt(salt)? })
    }

    /// The block of private input words: `[n, id_lo, id_hi, k0_lo, k0_hi, …, s0, …, s7]`, absent
    /// keys as zeros.
    pub fn block(&self) -> Result<[u32; BLOCK], String> {
        if self.keys.len() > MAX {
            return Err(format!("{} keys; a list holds at most {MAX}", self.keys.len()));
        }
        let mut b = [0u32; BLOCK];
        b[0] = self.keys.len() as u32;
        b[1] = self.id as u32;
        b[2] = (self.id >> 32) as u32;
        for (i, k) in self.keys.iter().enumerate() {
            b[3 + 2 * i] = *k as u32;
            b[4 + 2 * i] = (*k >> 32) as u32;
        }
        b[3 + 2 * MAX..].copy_from_slice(&self.salt);
        Ok(b)
    }
}

/// A list file: whitespace-separated keys, decimal or `0x` hex, in file order; `#` starts a
/// comment; a line `self <key>` names the party's own id (default 0).
pub fn parse_list(text: &str) -> Result<(u64, Vec<u64>), String> {
    let mut id = None;
    let mut keys = vec![];
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut toks = line.split_whitespace();
        let first = toks.next().unwrap_or("");
        if first == "self" {
            if id.is_some() {
                return Err(format!("line {}: a second `self`", ln + 1));
            }
            let v = toks.next().ok_or_else(|| format!("line {}: `self` needs a key", ln + 1))?;
            id = Some(parse_key(v).map_err(|e| format!("line {}: {e}", ln + 1))?);
            if toks.next().is_some() {
                return Err(format!("line {}: more after `self <key>`", ln + 1));
            }
            continue;
        }
        for t in std::iter::once(first).chain(toks) {
            keys.push(parse_key(t).map_err(|e| format!("line {}: {e}", ln + 1))?);
        }
    }
    Ok((id.unwrap_or(0), keys))
}

/// A key: decimal or `0x` hex, below 2^64.
pub fn parse_key(t: &str) -> Result<u64, String> {
    match t.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => t.parse(),
    }
    .map_err(|_| format!("not a 64-bit key: {t}"))
}

/// Eight decimal words from a salt file.
pub fn read_salt(path: &str) -> Result<[u32; 8], String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let v: Vec<u32> = text
        .split_whitespace()
        .map(|t| t.parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{path}: {e}"))?;
    v.try_into().map_err(|_| format!("{path}: not eight words"))
}

/// `[blind, blind, A's block, B's block]`: a call's private inputs.
pub fn input_words(blinds: [u32; 2], a: &[u32; BLOCK], b: &[u32; BLOCK]) -> Vec<u32> {
    let mut w = blinds.to_vec();
    w.extend_from_slice(a);
    w.extend_from_slice(b);
    w
}

/// `[C_A, C_B, mode]`: the deploy-time public input.
pub fn public_words(c_a: &[u32; 8], c_b: &[u32; 8], mode: u32) -> Vec<u32> {
    let mut w = c_a.to_vec();
    w.extend_from_slice(c_b);
    w.push(mode);
    debug_assert_eq!(w.len(), PUBLIC_WORDS as usize);
    w
}

/// The real `POSEIDON2`, by running `commit/image.bin` on the emulator. Needs `$CIRCUITS` (and
/// `rand-guest` built there); the image is `$JOIN_COMMIT_IMAGE`, default `../commit/image.bin`
/// beside this crate.
pub struct Emulator {
    rg: PathBuf,
    image: PathBuf,
    memo: RefCell<HashMap<[u32; BLOCK], [u32; 8]>>,
}

impl Emulator {
    pub fn new() -> Result<Emulator, String> {
        let circuits = std::env::var("CIRCUITS").map_err(|_| "CIRCUITS is not set (see scripts/env.sh)".to_string())?;
        let rg = PathBuf::from(&circuits).join("rand-guest/target/release/rand-guest");
        if !rg.is_file() {
            return Err(format!("no rand-guest at {} (run ./build.sh)", rg.display()));
        }
        let image = std::env::var("JOIN_COMMIT_IMAGE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commit/image.bin"));
        if !image.is_file() {
            return Err(format!("no commit image at {} (run ./build.sh)", image.display()));
        }
        Ok(Emulator { rg, image, memo: RefCell::new(HashMap::new()) })
    }

    /// A block's commitment, `POSEIDON2([TAG, block…])`, by the guest's own code.
    pub fn commitment(&self, block: &[u32; BLOCK]) -> [u32; 8] {
        if let Some(d) = self.memo.borrow().get(block) {
            return *d;
        }
        let mut cmd = Command::new(&self.rg);
        cmd.arg("run").arg(&self.image).arg("--input");
        for w in block {
            cmd.arg(w.to_string());
        }
        let out = cmd.output().unwrap_or_else(|e| fail(&format!("running {}: {e}", self.rg.display())));
        let text = String::from_utf8_lossy(&out.stdout);
        let d = parse_outputs(&text).unwrap_or_else(|| {
            fail(&format!("commit/image.bin did not print eight words:\n{text}{}", String::from_utf8_lossy(&out.stderr)))
        });
        self.memo.borrow_mut().insert(*block, d);
        d
    }

    /// As a [`Hasher`]: the message must carry the tag the committer puts there itself.
    pub fn hash(&self, m: [u32; MSG]) -> [u32; 8] {
        if m[0] != TAG {
            fail("the message does not start with TAG");
        }
        let mut block = [0u32; BLOCK];
        block.copy_from_slice(&m[1..]);
        self.commitment(&block)
    }
}

/// `out[i] = w` lines from `rand-guest run`, as the eight words.
fn parse_outputs(text: &str) -> Option<[u32; 8]> {
    let mut d = [0u32; 8];
    let mut seen = 0;
    for line in text.lines() {
        if let Some((i, v)) = line.strip_prefix("out[").and_then(|r| r.split_once("] = ")) {
            if let (Ok(i), Ok(v)) = (i.parse::<usize>(), v.trim().parse::<u32>()) {
                if i < 8 {
                    d[i] = v;
                    seen += 1;
                }
            }
        }
    }
    (seen == 8).then_some(d)
}

/// A stand-in hash for unit tests: deterministic and input-sensitive, **not** Poseidon2. Tests
/// compute every expected commitment with it; the real hash is exercised by `run.sh`.
pub fn test_hash(msg: [u32; MSG]) -> [u32; 8] {
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

/// A [`Source`] over vectors. Reading past the end of either is what the guest cannot do — on
/// the zkVM it is an unsatisfiable request, so no proof — and is recorded in `oob`.
pub struct Mock<'a> {
    pub public: Vec<u32>,
    pub input: Vec<u32>,
    pub hasher: Hasher<'a>,
    pub oob: Cell<bool>,
}

impl<'a> Mock<'a> {
    pub fn new(public: Vec<u32>, input: Vec<u32>, hasher: Hasher<'a>) -> Mock<'a> {
        Mock { public, input, hasher, oob: Cell::new(false) }
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

    /// What the guest would do: its outputs, or why it refuses — `None` when it read a word the
    /// caller never committed, which on the zkVM traps at the read, before any later rule.
    pub fn accepts(&self) -> Result<[u32; 8], Option<Refusal>> {
        let r = crate::check(self);
        if self.oob.get() {
            return Err(None);
        }
        r.map_err(Some)
    }
}

impl Source for Mock<'_> {
    fn input(&self, i: u32) -> u32 {
        self.get(&self.input, i)
    }
    fn public(&self, i: u32) -> u32 {
        self.get(&self.public, i)
    }
    fn hash(&self, buf: &mut [u32; MSG]) {
        let d = (self.hasher)(*buf);
        buf[..8].copy_from_slice(&d);
    }
}

pub fn join(w: &[u32]) -> String {
    w.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

/// Print `error: …` and exit 1.
pub fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}
