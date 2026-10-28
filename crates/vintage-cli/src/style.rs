//! Terminal styling for human output: the wordmark, dim labels, gold for proven results.
//! Plain text when stdout is not a terminal or NO_COLOR is set, so pipes and --json stay clean.

use std::io::IsTerminal;
use std::sync::OnceLock;

const GOLD: &str = "\x1b[1;38;2;244;183;40m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

fn paint(code: &str, s: &str) -> String {
    if on() {
        format!("{code}{s}{RESET}")
    } else {
        s.to_string()
    }
}

/// The VINTAGE wordmark and tagline, only on a terminal.
pub fn banner(network: &str) {
    if !on() {
        return;
    }
    println!();
    println!("  {GOLD}█ █ █ █▄ █ ▀█▀ ▄▀█ █▀▀ █▀▀{RESET}");
    println!("  {GOLD}▀▄▀ █ █ ▀█  █  █▀█ █▄█ ██▄{RESET}");
    let network = match network {
        "main" => "Zcash mainnet",
        "test" => "Zcash testnet",
        other => other,
    };
    println!("  {DIM}◆ proof of age for private money · {network}{RESET}");
    println!();
}

/// A field label such as "txid:        ", dimmed.
pub fn key(label: &str) -> String {
    paint(DIM, label)
}

/// A proven fact (a tick, an age): gold.
pub fn gold(s: &str) -> String {
    paint(GOLD, s)
}

/// The verdict line: gold for a proven result, bold for a fail, dim otherwise. The text is unchanged.
pub fn verdict(verdict: &str, summary: &str) -> String {
    let code = match verdict {
        "PASS" | "DATED" | "LINK PASS" => GOLD,
        "FAIL" | "LINK FAIL" => BOLD,
        _ => DIM,
    };
    format!("{}: {summary}", paint(code, verdict))
}
