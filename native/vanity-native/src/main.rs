use chrono::Local;
use rand::rngs::OsRng;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use sha3::{Digest, Keccak256};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone)]
struct Config {
    prefix: String,
    suffix: String,
    prefix_nibbles: Vec<u8>,
    suffix_nibbles: Vec<u8>,
    workers: usize,
    status_interval: Duration,
    batch_size: usize,
    case_sensitive: bool,
    redact_private_key: bool,
    plain_output: bool,
    max_seconds: u64,
    state_dir: PathBuf,
    result_dir: PathBuf,
    logs_dir: PathBuf,
}

struct ThreadState {
    attempts: AtomicU64,
}

struct MatchResult {
    address: String,
    private_key: String,
    worker_id: usize,
    worker_attempts: u64,
    found_at: String,
}

struct StatusSnapshot {
    attempts: u64,
    rate: u64,
    runtime: String,
    workers: usize,
    matched: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = Arc::new(parse_args()?);
    fs::create_dir_all(&config.state_dir).map_err(|e| format!("create state dir failed: {e}"))?;
    fs::create_dir_all(&config.result_dir).map_err(|e| format!("create result dir failed: {e}"))?;
    fs::create_dir_all(&config.logs_dir).map_err(|e| format!("create logs dir failed: {e}"))?;

    let run_id = Local::now().format("%Y%m%d-%H%M%S%3f").to_string();
    log_event(
        &config,
        &format!(
            "run {run_id} started prefix={} suffix={} workers={}",
            display_pattern(&config.prefix),
            display_pattern(&config.suffix),
            config.workers
        ),
    )?;
    print_banner(&config, &run_id);

    let stop = Arc::new(AtomicBool::new(false));
    let interrupted = Arc::new(AtomicBool::new(false));
    let found = Arc::new(AtomicBool::new(false));
    let match_result = Arc::new(Mutex::new(None::<MatchResult>));
    let states = Arc::new(
        (0..config.workers)
            .map(|_| ThreadState {
                attempts: AtomicU64::new(0),
            })
            .collect::<Vec<_>>(),
    );

    {
        let stop = Arc::clone(&stop);
        let interrupted = Arc::clone(&interrupted);
        ctrlc::set_handler(move || {
            interrupted.store(true, Ordering::SeqCst);
            stop.store(true, Ordering::SeqCst);
        })
        .map_err(|e| format!("failed to install Ctrl+C handler: {e}"))?;
    }

    let mut handles = Vec::with_capacity(config.workers);
    for worker_index in 0..config.workers {
        let worker_config = Arc::clone(&config);
        let worker_stop = Arc::clone(&stop);
        let worker_found = Arc::clone(&found);
        let worker_result = Arc::clone(&match_result);
        let worker_states = Arc::clone(&states);

        handles.push(thread::spawn(move || {
            worker_loop(
                worker_index + 1,
                worker_config,
                worker_stop,
                worker_found,
                worker_result,
                worker_states,
            );
        }));
    }

    let started = Instant::now();
    let mut next_status = Instant::now() + config.status_interval;
    let mut last_attempts = 0u64;
    let mut last_status_at = Instant::now();
    let mut last_live_len = 0usize;
    let mut last_rate = 0u64;
    let stop_reason: String;

    loop {
        if found.load(Ordering::Relaxed) {
            stop_reason = String::from("match found");
            break;
        }

        if stop.load(Ordering::Relaxed) {
            stop_reason = if interrupted.load(Ordering::Relaxed) {
                String::from("interrupted by Ctrl+C")
            } else {
                String::from("stopped")
            };
            break;
        }

        if config.max_seconds > 0 && started.elapsed().as_secs() >= config.max_seconds {
            stop_reason = format!("max run time reached after {}s", config.max_seconds);
            stop.store(true, Ordering::SeqCst);
            break;
        }

        if Instant::now() >= next_status {
            let attempts = total_attempts(&states);
            let elapsed = last_status_at.elapsed().as_secs_f64().max(0.001);
            last_rate = ((attempts.saturating_sub(last_attempts)) as f64 / elapsed).round() as u64;
            last_attempts = attempts;
            last_status_at = Instant::now();

            let snapshot = StatusSnapshot {
                attempts,
                rate: last_rate,
                runtime: format_duration(started.elapsed()),
                workers: config.workers,
                matched: false,
            };

            write_status(&config, &run_id, &snapshot)?;
            print_status(&config, &snapshot, &mut last_live_len)?;
            next_status += config.status_interval;
        }

        thread::sleep(Duration::from_millis(50));
    }

    stop.store(true, Ordering::SeqCst);
    for handle in handles {
        let _ = handle.join();
    }

    finish_live_line(&config, &mut last_live_len)?;

    let final_attempts = total_attempts(&states);
    let final_snapshot = StatusSnapshot {
        attempts: final_attempts,
        rate: last_rate,
        runtime: format_duration(started.elapsed()),
        workers: config.workers,
        matched: found.load(Ordering::Relaxed),
    };
    write_status(&config, &run_id, &final_snapshot)?;

    let result = match_result
        .lock()
        .map_err(|_| "match result lock poisoned".to_string())?
        .take();
    if let Some(result) = result {
        let result_path = write_result(&config, &run_id, &result, final_attempts)?;
        log_event(
            &config,
            &format!(
                "match found address={} result={}",
                result.address,
                result_path.display()
            ),
        )?;
        println!("MATCH FOUND");
        println!("Address: {}", result.address);
        println!("Result:  {}", result_path.display());
    } else {
        log_event(
            &config,
            &format!("run {run_id} stopping: {stop_reason} attempts={final_attempts}"),
        )?;
        println!("Stopped: {stop_reason}");
        println!("Attempts: {}", format_number(final_attempts));
    }

    if interrupted.load(Ordering::Relaxed) {
        std::process::exit(130);
    }

    Ok(())
}

fn worker_loop(
    worker_id: usize,
    config: Arc<Config>,
    stop: Arc<AtomicBool>,
    found: Arc<AtomicBool>,
    match_result: Arc<Mutex<Option<MatchResult>>>,
    states: Arc<Vec<ThreadState>>,
) {
    let secp = Secp256k1::new();
    let mut seed = <ChaCha20Rng as SeedableRng>::Seed::default();
    OsRng.fill_bytes(&mut seed);
    let mut rng = ChaCha20Rng::from_seed(seed);
    let mut private_key = [0u8; 32];
    let mut attempts = 0u64;

    while !stop.load(Ordering::Relaxed) && !found.load(Ordering::Relaxed) {
        for _ in 0..config.batch_size {
            if stop.load(Ordering::Relaxed) || found.load(Ordering::Relaxed) {
                break;
            }

            rng.fill_bytes(&mut private_key);
            attempts = attempts.wrapping_add(1);

            let secret_key = match SecretKey::from_byte_array(private_key) {
                Ok(secret_key) => secret_key,
                Err(_) => continue,
            };

            let public_key = PublicKey::from_secret_key(&secp, &secret_key);
            let serialized = public_key.serialize_uncompressed();
            let hash = keccak(&serialized[1..]);

            if !matches_address_hash(&hash, &config) {
                continue;
            }

            let address_body = last20_to_hex(&hash);
            let comparable_body = if config.case_sensitive {
                checksum_body(&address_body)
            } else {
                address_body.clone()
            };

            if !matches_address_body(&comparable_body, &config) {
                continue;
            }

            states[worker_id - 1]
                .attempts
                .store(attempts, Ordering::Relaxed);
            if !found.swap(true, Ordering::SeqCst) {
                stop.store(true, Ordering::SeqCst);
                let address = format!("0x{}", checksum_body(&address_body));
                let result = MatchResult {
                    address,
                    private_key: format!("0x{}", bytes_to_hex(&private_key)),
                    worker_id,
                    worker_attempts: attempts,
                    found_at: Local::now().to_rfc3339(),
                };

                if let Ok(mut slot) = match_result.lock() {
                    *slot = Some(result);
                }
            }

            return;
        }

        states[worker_id - 1]
            .attempts
            .store(attempts, Ordering::Relaxed);
    }

    states[worker_id - 1]
        .attempts
        .store(attempts, Ordering::Relaxed);
}

fn parse_args() -> Result<Config, String> {
    let mut prefix = String::new();
    let mut suffix = String::from("00000000");
    let mut workers = usize::max(1, num_cpus::get().saturating_sub(1));
    let mut status_interval_seconds = 5u64;
    let mut batch_size = 1024usize;
    let mut case_sensitive = false;
    let mut redact_private_key = false;
    let mut plain_output = false;
    let mut max_seconds = 0u64;
    let mut state_dir = PathBuf::from("state");
    let mut result_dir = PathBuf::from("results");
    let mut logs_dir = PathBuf::from("logs");

    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        let key = args[index].as_str();
        match key {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--prefix" => prefix = next_value(&args, &mut index, key)?,
            "--suffix" => suffix = next_value(&args, &mut index, key)?,
            "--workers" => {
                workers = parse_positive::<usize>(&next_value(&args, &mut index, key)?, key)?
            }
            "--status-interval" => {
                status_interval_seconds =
                    parse_positive::<u64>(&next_value(&args, &mut index, key)?, key)?
            }
            "--batch-size" => {
                batch_size = parse_positive::<usize>(&next_value(&args, &mut index, key)?, key)?
            }
            "--max-seconds" => {
                max_seconds = parse_positive::<u64>(&next_value(&args, &mut index, key)?, key)?
            }
            "--state-dir" => state_dir = PathBuf::from(next_value(&args, &mut index, key)?),
            "--result-dir" => result_dir = PathBuf::from(next_value(&args, &mut index, key)?),
            "--logs-dir" => logs_dir = PathBuf::from(next_value(&args, &mut index, key)?),
            "--case-sensitive" => case_sensitive = true,
            "--redact-private-key" => redact_private_key = true,
            "--plain-output" => plain_output = true,
            other => return Err(format!("unknown argument: {other}")),
        }

        index += 1;
    }

    prefix = normalize_hex_pattern(&prefix, "prefix", case_sensitive)?;
    suffix = normalize_hex_pattern(&suffix, "suffix", case_sensitive)?;

    if prefix.is_empty() && suffix.is_empty() {
        return Err("at least one of prefix or suffix is required".to_string());
    }

    if prefix.len() + suffix.len() > 40 {
        return Err(
            "prefix plus suffix cannot exceed 40 hex characters for an EVM address".to_string(),
        );
    }

    Ok(Config {
        prefix_nibbles: hex_to_nibbles(&prefix)?,
        suffix_nibbles: hex_to_nibbles(&suffix)?,
        prefix,
        suffix,
        workers,
        status_interval: Duration::from_secs(status_interval_seconds),
        batch_size,
        case_sensitive,
        redact_private_key,
        plain_output,
        max_seconds,
        state_dir,
        result_dir,
        logs_dir,
    })
}

fn next_value(args: &[String], index: &mut usize, key: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{key} requires a value"))
}

fn parse_positive<T>(value: &str, key: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + From<u8>,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| format!("{key} must be a positive integer"))?;
    if parsed < T::from(1) {
        return Err(format!("{key} must be a positive integer"));
    }
    Ok(parsed)
}

fn normalize_hex_pattern(value: &str, name: &str, preserve_case: bool) -> Result<String, String> {
    let mut normalized = value.trim().to_string();
    if normalized.to_ascii_lowercase().starts_with("0x") {
        normalized = normalized[2..].to_string();
    }

    if !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} must contain only hexadecimal characters, optionally prefixed by 0x"
        ));
    }

    if preserve_case {
        Ok(normalized)
    } else {
        Ok(normalized.to_ascii_lowercase())
    }
}

fn hex_to_nibbles(value: &str) -> Result<Vec<u8>, String> {
    value
        .bytes()
        .map(|byte| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err("invalid hex character".to_string()),
        })
        .collect()
}

fn matches_address_hash(hash: &[u8; 32], config: &Config) -> bool {
    for (index, expected) in config.prefix_nibbles.iter().enumerate() {
        if address_nibble(hash, index) != *expected {
            return false;
        }
    }

    let suffix_start = 40usize.saturating_sub(config.suffix_nibbles.len());
    for (index, expected) in config.suffix_nibbles.iter().enumerate() {
        if address_nibble(hash, suffix_start + index) != *expected {
            return false;
        }
    }

    true
}

fn matches_address_body(address_body: &str, config: &Config) -> bool {
    (config.prefix.is_empty() || address_body.starts_with(&config.prefix))
        && (config.suffix.is_empty() || address_body.ends_with(&config.suffix))
}

fn address_nibble(hash: &[u8; 32], address_nibble_index: usize) -> u8 {
    let byte = hash[12 + (address_nibble_index >> 1)];
    if address_nibble_index % 2 == 0 {
        byte >> 4
    } else {
        byte & 0x0f
    }
}

fn keccak(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn last20_to_hex(hash: &[u8; 32]) -> String {
    let mut output = String::with_capacity(40);
    for byte in &hash[12..32] {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn checksum_body(address_body: &str) -> String {
    let hash = keccak(address_body.as_bytes());
    let mut output = String::with_capacity(address_body.len());

    for (index, byte) in address_body.bytes().enumerate() {
        let hash_byte = hash[index >> 1];
        let nibble = if index % 2 == 0 {
            hash_byte >> 4
        } else {
            hash_byte & 0x0f
        };

        if (b'a'..=b'f').contains(&byte) && nibble >= 8 {
            output.push((byte - 32) as char);
        } else {
            output.push(byte as char);
        }
    }

    output
}

fn print_banner(config: &Config, run_id: &str) {
    let digits = config.prefix.len() + config.suffix.len();
    println!("Native EVM vanity search");
    println!("Run ID: {run_id}");
    println!(
        "Target: prefix '{}' suffix '{}'",
        display_pattern(&config.prefix),
        display_pattern(&config.suffix)
    );
    println!("Workers: {}", config.workers);
    println!(
        "Average attempts estimate: {}",
        average_attempts_text(digits)
    );
    if !config.plain_output {
        println!("Status updates will refresh on one line. Use -PlainOutput for scrolling output.");
    }
}

fn print_status(
    config: &Config,
    snapshot: &StatusSnapshot,
    last_live_len: &mut usize,
) -> Result<(), String> {
    let line = format!(
        "[{}] attempts={} rate={}/s runtime={} workers={}/{}",
        Local::now().format("%H:%M:%S"),
        format_number(snapshot.attempts),
        format_number(snapshot.rate),
        snapshot.runtime,
        snapshot.workers,
        config.workers
    );

    if config.plain_output {
        println!("{line}");
    } else {
        let padding = last_live_len.saturating_sub(line.len());
        print!("\r{line}{}", " ".repeat(padding));
        io::stdout()
            .flush()
            .map_err(|e| format!("flush stdout failed: {e}"))?;
        *last_live_len = line.len();
    }

    Ok(())
}

fn finish_live_line(config: &Config, last_live_len: &mut usize) -> Result<(), String> {
    if config.plain_output || *last_live_len == 0 {
        return Ok(());
    }

    print!("\r{}\r\n", " ".repeat(*last_live_len));
    io::stdout()
        .flush()
        .map_err(|e| format!("flush stdout failed: {e}"))?;
    *last_live_len = 0;
    Ok(())
}

fn write_status(config: &Config, run_id: &str, snapshot: &StatusSnapshot) -> Result<(), String> {
    let json = format!(
        concat!(
            "{{\n",
            "  \"runId\": \"{}\",\n",
            "  \"updatedAt\": \"{}\",\n",
            "  \"matched\": {},\n",
            "  \"engine\": \"native-rust\",\n",
            "  \"pattern\": {{\n",
            "    \"prefix\": \"{}\",\n",
            "    \"suffix\": \"{}\",\n",
            "    \"caseSensitive\": {}\n",
            "  }},\n",
            "  \"totalAttempts\": {},\n",
            "  \"totalRatePerSecond\": {},\n",
            "  \"runtime\": \"{}\",\n",
            "  \"aliveWorkers\": {},\n",
            "  \"configuredWorkers\": {},\n",
            "  \"totalRestarts\": 0\n",
            "}}\n"
        ),
        run_id,
        Local::now().to_rfc3339(),
        snapshot.matched,
        config.prefix,
        config.suffix,
        config.case_sensitive,
        snapshot.attempts,
        snapshot.rate,
        snapshot.runtime,
        snapshot.workers,
        config.workers,
    );

    atomic_write(&config.state_dir.join("status.json"), json.as_bytes())
}

fn write_result(
    config: &Config,
    run_id: &str,
    result: &MatchResult,
    total_attempts: u64,
) -> Result<PathBuf, String> {
    let result_path = config
        .result_dir
        .join(format!("matched-wallet-native-{run_id}.txt"));
    let private_key = if config.redact_private_key {
        String::from("[redacted by --redact-private-key]")
    } else {
        result.private_key.clone()
    };

    let body = format!(
        concat!(
            "EVM Vanity Wallet Match\n\n",
            "Engine: native-rust\n",
            "RunId: {}\n",
            "FoundAt: {}\n",
            "Address: {}\n",
            "PrivateKey: {}\n",
            "Prefix: {}\n",
            "Suffix: {}\n",
            "CaseSensitive: {}\n",
            "EstimatedAverageAttempts: {}\n",
            "TotalAttemptsObserved: {}\n",
            "WorkerId: {}\n",
            "WorkerAttemptsThisRun: {}\n\n",
            "Security notes:\n",
            "- Keep the private key offline and never paste it into websites.\n",
            "- Fund the address only after you have backed up the private key.\n",
            "- Anyone who sees this private key can spend funds from this address.\n"
        ),
        run_id,
        result.found_at,
        result.address,
        private_key,
        display_pattern(&config.prefix),
        display_pattern(&config.suffix),
        config.case_sensitive,
        average_attempts_text(config.prefix.len() + config.suffix.len()),
        total_attempts,
        result.worker_id,
        result.worker_attempts,
    );

    atomic_write(&result_path, body.as_bytes())?;
    atomic_write(
        &config.result_dir.join("matched-wallet-latest.txt"),
        body.as_bytes(),
    )?;
    Ok(result_path)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create dir {} failed: {e}", parent.display()))?;
    }

    let temp_path = path.with_file_name(format!(
        "{}.{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        Local::now().timestamp_nanos_opt().unwrap_or_default()
    ));

    fs::write(&temp_path, bytes)
        .map_err(|e| format!("write {} failed: {e}", temp_path.display()))?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("remove {} failed: {e}", path.display()))?;
    }
    fs::rename(&temp_path, path).map_err(|e| {
        format!(
            "rename {} to {} failed: {e}",
            temp_path.display(),
            path.display()
        )
    })
}

fn log_event(config: &Config, message: &str) -> Result<(), String> {
    fs::create_dir_all(&config.logs_dir).map_err(|e| format!("create logs dir failed: {e}"))?;
    let log_path = config
        .logs_dir
        .join(format!("{}.log", Local::now().format("%Y-%m-%d")));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("open log {} failed: {e}", log_path.display()))?;
    writeln!(file, "[{}] {}", Local::now().to_rfc3339(), message)
        .map_err(|e| format!("write log failed: {e}"))
}

fn total_attempts(states: &[ThreadState]) -> u64 {
    states
        .iter()
        .map(|state| state.attempts.load(Ordering::Relaxed))
        .sum()
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn format_number(value: u64) -> String {
    let text = value.to_string();
    let mut output = String::with_capacity(text.len() + text.len() / 3);
    for (index, character) in text.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output.chars().rev().collect()
}

fn display_pattern(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}

fn average_attempts_text(digits: usize) -> String {
    average_attempts_plain(digits)
        .map(|value| format_number(value))
        .unwrap_or_else(|| format!("16^{digits}"))
}

fn average_attempts_plain(digits: usize) -> Option<u64> {
    let mut value = 1u64;
    for _ in 0..digits {
        value = value.checked_mul(16)?;
    }
    Some(value)
}

fn print_help() {
    println!("vanity-native.exe --suffix 00000000 --workers 8");
    println!();
    println!("Options:");
    println!("  --prefix <hex>");
    println!("  --suffix <hex>");
    println!("  --workers <number>");
    println!("  --status-interval <seconds>");
    println!("  --batch-size <number>");
    println!("  --max-seconds <seconds>");
    println!("  --state-dir <path>");
    println!("  --result-dir <path>");
    println!("  --logs-dir <path>");
    println!("  --case-sensitive");
    println!("  --redact-private-key");
    println!("  --plain-output");
}
