//! Host side (never on the guest): the roll and ballots files, the call's words, a [`Mock`]
//! source so the tests and the `ballot` tool run the very rules the guest runs, and the roll's
//! real digest computed by the `fold` helper guest on the zkVM's emulator.

use crate::{check, seed, step, Hash, Refusal, Source, MAX_OPTIONS, MAX_VOTERS, MIN_OPTIONS};
use std::cell::Cell;
use std::path::PathBuf;
use std::process::Command;

/// The eligible-voter roll: how many options the ballot has and `(voter_tag, weight)` per voter,
/// in roll order. This is what the organiser publishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roll {
    pub n_options: u32,
    pub voters: Vec<(u32, u64)>,
}

impl Roll {
    pub fn n(&self) -> u32 {
        self.voters.len() as u32
    }

    pub fn total(&self) -> u64 {
        self.voters.iter().map(|v| v.1).sum()
    }

    /// The roll file: `options <n>` once, then one `<voter_tag> <weight>` per line; `#` comments.
    pub fn parse(text: &str) -> Result<Roll, String> {
        let mut n_options = None;
        let mut voters = vec![];
        for (ln, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let at = |e: &str| format!("roll line {}: {e}: {raw}", ln + 1);
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["options", n] => {
                    let n: u32 = n.parse().map_err(|_| at("not a count"))?;
                    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&n) {
                        return Err(at(&format!("options must be {MIN_OPTIONS}..={MAX_OPTIONS}")));
                    }
                    if n_options.replace(n).is_some() {
                        return Err(at("options given twice"));
                    }
                }
                [tag, w] => {
                    let tag: u32 = tag.parse().map_err(|_| at("voter tag: not a u32"))?;
                    let w: u64 = w.parse().map_err(|_| at("weight: not a u64"))?;
                    if w >> 63 != 0 {
                        return Err(at("weight must be below 2^63"));
                    }
                    if voters.iter().any(|v: &(u32, u64)| v.0 == tag) {
                        return Err(at("voter tag already on the roll"));
                    }
                    voters.push((tag, w));
                }
                _ => return Err(at("expected `options <n>` or `<voter_tag> <weight>`")),
            }
        }
        let n_options = n_options.ok_or("roll: no `options <n>` line")?;
        if voters.is_empty() {
            return Err("roll: no voters".into());
        }
        if voters.len() > MAX_VOTERS as usize {
            return Err(format!("roll: {} voters; the program takes at most {MAX_VOTERS}", voters.len()));
        }
        let total: u128 = voters.iter().map(|v| v.1 as u128).sum();
        if total >> 63 != 0 {
            return Err("roll: the weights sum to 2^63 or more".into());
        }
        Ok(Roll { n_options, voters })
    }

    /// The ballots file: one `<voter_tag> <choice>` per line, `choice` an option index or
    /// `abstain`; `#` comments. A voter with no line abstains. Returns a choice per voter in roll
    /// order, abstention spelled as `n_options` (any choice `>= n_options` is one to the program).
    pub fn ballots(&self, text: &str) -> Result<Vec<u32>, String> {
        let mut choice: Vec<Option<u32>> = vec![None; self.voters.len()];
        for (ln, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let at = |e: &str| format!("ballots line {}: {e}: {raw}", ln + 1);
            let f: Vec<&str> = line.split_whitespace().collect();
            let [tag, c] = f.as_slice() else { return Err(at("expected `<voter_tag> <choice|abstain>`")) };
            let tag: u32 = tag.parse().map_err(|_| at("voter tag: not a u32"))?;
            let c: u32 = if *c == "abstain" { self.n_options } else { c.parse().map_err(|_| at("choice: not a u32"))? };
            let i = self.voters.iter().position(|v| v.0 == tag).ok_or_else(|| at("not on the roll"))?;
            if choice[i].replace(c).is_some() {
                return Err(at("this voter already has a ballot"));
            }
        }
        Ok(choice.into_iter().map(|c| c.unwrap_or(self.n_options)).collect())
    }

    /// The `fold` helper guest's inputs: `[n, (voter_tag, w_lo, w_hi) × n]`.
    pub fn fold_words(&self) -> Vec<u32> {
        let mut w = vec![self.n()];
        for (tag, weight) in &self.voters {
            w.extend_from_slice(&[*tag, *weight as u32, (*weight >> 32) as u32]);
        }
        w
    }

    /// The call's private inputs after the two blind words: `[n, (voter_tag, w_lo, w_hi, choice) × n]`.
    pub fn input_words(&self, choices: &[u32]) -> Vec<u32> {
        let mut w = vec![self.n()];
        for ((tag, weight), c) in self.voters.iter().zip(choices) {
            w.extend_from_slice(&[*tag, *weight as u32, (*weight >> 32) as u32, *c]);
        }
        w
    }

    /// The deploy's public input, given the roll's digest: `[n_options, R0..R7]`.
    pub fn public_words(&self, r: &[u32; 8]) -> Vec<u32> {
        let mut w = vec![self.n_options];
        w.extend_from_slice(r);
        w
    }

    /// `R` under any hash: the program's own fold, on the host.
    pub fn digest<H: Hash>(&self, h: &H) -> [u32; 8] {
        let mut st = seed(self.n());
        for (tag, weight) in &self.voters {
            st = step(h, st, *tag, *weight as u32, (*weight >> 32) as u32);
        }
        st
    }

    /// `R` under the real Poseidon2: the `fold` helper guest run on the emulator. Needs `CIRCUITS`
    /// (with `rand-guest` built there) and `FOLD_IMAGE`, the image `build.sh` makes.
    pub fn real_digest(&self) -> Result<[u32; 8], String> {
        let circuits = std::env::var("CIRCUITS").map_err(|_| "CIRCUITS is not set".to_string())?;
        let rg = PathBuf::from(&circuits).join("rand-guest/target/release/rand-guest");
        let image = std::env::var("FOLD_IMAGE").map_err(|_| "FOLD_IMAGE is not set (run the tool through the scripts)".to_string())?;
        let mut cmd = Command::new(&rg);
        cmd.arg("run").arg(&image).arg("--input");
        for w in self.fold_words() {
            cmd.arg(w.to_string());
        }
        let out = cmd.output().map_err(|e| format!("running {}: {e}", rg.display()))?;
        let text = String::from_utf8_lossy(&out.stdout);
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
        if seen != 8 {
            return Err(format!("fold did not print eight words:\n{text}{}", String::from_utf8_lossy(&out.stderr)));
        }
        Ok(d)
    }

    /// What the program will publish for these choices, and what it will not: the abstentions.
    pub fn expected(&self, choices: &[u32]) -> ([u64; 4], u64) {
        let mut t = [0u64; 4];
        let mut abstain = 0;
        for ((_, w), c) in self.voters.iter().zip(choices) {
            if *c < self.n_options {
                t[*c as usize] += w;
            } else {
                abstain += w;
            }
        }
        (t, abstain)
    }
}

/// A [`Source`] over vectors. Reading past the end of either is what the guest cannot do — on the
/// zkVM it is an unsatisfiable request, so no proof — and is recorded in `oob`.
pub struct Mock {
    pub public: Vec<u32>,
    pub input: Vec<u32>,
    pub hasher: fn([u32; 12]) -> [u32; 8],
    pub oob: Cell<bool>,
}

impl Mock {
    pub fn new(public: &[u32], input: &[u32], hasher: fn([u32; 12]) -> [u32; 8]) -> Mock {
        Mock { public: public.to_vec(), input: input.to_vec(), hasher, oob: Cell::new(false) }
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

    /// Run the rules and say what the guest would do: the outputs, or why there is no proof.
    pub fn run(&self) -> Result<[u32; 8], Refusal> {
        let r = check(self);
        if self.oob.get() {
            // The guest asked for a word nobody committed: unsatisfiable, whatever `check` said.
            return Err(Refusal::Voters);
        }
        r
    }
}

impl Hash for Mock {
    fn hash12(&self, msg: [u32; 12]) -> [u32; 8] {
        (self.hasher)(msg)
    }
}

impl Source for Mock {
    fn public(&self, i: u32) -> u32 {
        self.get(&self.public, i)
    }
    fn input(&self, i: u32) -> u32 {
        self.get(&self.input, i)
    }
}

/// A stand-in hash for the tests and the tool's expected tally: deterministic and sensitive to
/// every word, **not** Poseidon2. The real hash runs in `run.sh`, on the emulator.
pub fn test_hash(msg: [u32; 12]) -> [u32; 8] {
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

/// A [`Hash`] over a plain function, for [`Roll::digest`] in tests.
pub struct Fn12(pub fn([u32; 12]) -> [u32; 8]);

impl Hash for Fn12 {
    fn hash12(&self, msg: [u32; 12]) -> [u32; 8] {
        (self.0)(msg)
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
