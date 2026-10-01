//! score: what the receipt will say, before anything is proved.
//!   score <model file> <statements file>
//! Both files are whitespace-separated decimal words; `#` starts a comment. Exits 1 if the model is
//! one the guest would refuse, or the statements are not exactly twelve months.
use credit_score_core::{score, Model, Statements, BPS, MODEL_WORDS, MONTHS, STMT_WORDS};
use std::process::exit;

fn words(path: &str) -> Vec<u32> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
    text.lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(|l| l.split_whitespace().map(String::from).collect::<Vec<_>>())
        .map(|w| w.parse::<u32>().unwrap_or_else(|_| fail(&format!("{path}: `{w}` is not a u32 word"))))
        .collect()
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model_path, stmt_path] = args.as_slice() else { fail("usage: score <model file> <statements file>") };
    let m = words(model_path);
    let m: [u32; MODEL_WORDS] = m.as_slice().try_into().unwrap_or_else(|_| fail(&format!("{model_path}: a model is {MODEL_WORDS} words, not {}", m.len())));
    let model = Model::from_words(m).unwrap_or_else(|| fail("the model's DTI words are over 10 000 bps: the guest refuses every call"));
    let s = words(stmt_path);
    let s: [u32; STMT_WORDS] = s.as_slice().try_into().unwrap_or_else(|_| fail(&format!("{stmt_path}: twelve months of income, payment and end balance are {STMT_WORDS} words, not {}", s.len())));
    let sc = score(&model, &Statements::from_words(&s));

    println!("months covered   {} of {} (gate: at least {})", sc.months_positive, MONTHS, model.min_months_positive);
    let dti = if sc.sum_income == 0 { "n/a (no income)".to_string() } else { format!("{} bps", sc.sum_payment * BPS / sc.sum_income) };
    println!("income {}, payments {}: DTI {} (gate: at most {} bps; prime: at most {})", sc.sum_income, sc.sum_payment, dti, model.max_dti_bps, model.prime_dti_bps);
    println!("average income   {} (gate: at least {})", sc.sum_income / MONTHS as u64, model.min_monthly_income);
    println!("average balance  {} (gate: at least {}; prime: at least {})", sc.sum_balance / MONTHS as u64, model.min_avg_balance, model.prime_avg_balance);
    println!("band {}", sc.band);
}
