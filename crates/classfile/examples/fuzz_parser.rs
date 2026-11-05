//! Reproducible, seeded mutation fuzzing. No coverage-guidance claim is made.
use crabspot_classfile::{ParseOptions, parse_with_options};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_INPUT: usize = 64 * 1024;
const MAX_CORPUS: usize = 512;

struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

struct Config {
    corpus: PathBuf,
    failures: PathBuf,
    seed: u64,
    iterations: u64,
    seconds: Option<u64>,
}

fn config() -> Result<Config, String> {
    let mut args = env::args().skip(1);
    let mut config = Config {
        corpus: PathBuf::from(args.next().ok_or("usage: fuzz_parser <corpus> [--iterations N | --seconds N] [--seed N] [--failures DIR]")?),
        failures: PathBuf::from("target/fuzz-failures"),
        seed: 0x4352_4142_5350_4f54,
        iterations: 10_000,
        seconds: None,
    };
    let mut iterations_set = false;
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--iterations" => {
                config.iterations = value.parse().map_err(|_| "invalid iteration count")?;
                iterations_set = true;
            }
            "--seconds" => config.seconds = Some(value.parse().map_err(|_| "invalid duration")?),
            "--seed" => {
                config.seed = value
                    .parse()
                    .map_err(|_| "seed must be an unsigned decimal integer")?
            }
            "--failures" => config.failures = PathBuf::from(value),
            _ => return Err(format!("unknown option: {option}")),
        }
    }
    if config.seconds.is_some() && !iterations_set {
        config.iterations = u64::MAX;
    }
    if config.seed == 0 || config.iterations == 0 || config.seconds == Some(0) {
        return Err("seed, iterations and duration must be nonzero".to_owned());
    }
    Ok(config)
}

fn corpus_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            corpus_files(&entry.path(), files)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|value| value == "class")
        {
            if files.len() == MAX_CORPUS {
                return Err(format!(
                    "corpus contains more than {MAX_CORPUS} class files"
                ));
            }
            files.push(entry.path());
        }
    }
    Ok(())
}

fn mutation(random: &mut Random, original: &[u8], iteration: u64) -> Vec<u8> {
    let mut bytes = original.to_vec();
    match iteration % 6 {
        0 | 5 => {
            for _ in 0..1 + random.below(8) {
                if bytes.is_empty() {
                    break;
                }
                let position = random.below(bytes.len());
                if iteration % 6 == 5 && position + 4 <= bytes.len() {
                    // Length/index/count corruption, including u32::MAX.
                    let value = [0_u32, 1, 0xffff, u32::MAX][random.below(4)];
                    bytes[position..position + 4].copy_from_slice(&value.to_be_bytes());
                } else {
                    bytes[position] = random.next() as u8;
                }
            }
        }
        1 => {
            bytes.truncate(random.below(bytes.len() + 1));
        }
        2 => {
            let position = random.below(bytes.len() + 1);
            let count = (1 + random.below(32)).min(MAX_INPUT - bytes.len());
            bytes.splice(position..position, (0..count).map(|_| random.next() as u8));
        }
        3 => {
            let count = (1 + random.below(32)).min(MAX_INPUT - bytes.len());
            bytes.extend((0..count).map(|_| random.next() as u8));
        }
        _ => {
            bytes = (0..random.below(4096))
                .map(|_| random.next() as u8)
                .collect();
            if bytes.len() >= 8 && random.next() & 1 == 0 {
                bytes[..8].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65]);
            }
        }
    }
    bytes
}

fn save_failure(config: &Config, iteration: u64, bytes: &[u8]) -> Result<PathBuf, String> {
    fs::create_dir_all(&config.failures).map_err(|error| error.to_string())?;
    let path = config
        .failures
        .join(format!("seed-{}-case-{iteration}.class", config.seed));
    fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

fn run(config: Config) -> Result<(), String> {
    let options = ParseOptions {
        max_class_bytes: MAX_INPUT,
        ..ParseOptions::default()
    };
    let mut files = Vec::new();
    corpus_files(&config.corpus, &mut files)?;
    if files.is_empty() {
        return Err("corpus contains no .class files".to_owned());
    }
    let mut corpus = Vec::new();
    for path in &files {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        parse_with_options(&bytes, options)
            .map_err(|error| format!("seed {}: {error}", path.display()))?;
        corpus.push(bytes);
    }
    let initial_count = corpus.len();
    let mut random = Random(config.seed);
    let start = Instant::now();
    let mut next_progress = Duration::from_secs(15);
    let mut executed = 0_u64;
    let mut accepted = 0_u64;
    let mut repeated = 0_u64;
    let deadline = config.seconds.map(Duration::from_secs);
    println!(
        "seed={} initial_corpus={} max_input={} max_annotation_depth={} max_annotation_values={}",
        config.seed,
        initial_count,
        MAX_INPUT,
        options.max_annotation_depth,
        options.max_annotation_values
    );
    while executed < config.iterations && deadline.is_none_or(|limit| start.elapsed() < limit) {
        let original = &corpus[random.below(corpus.len())];
        let bytes = mutation(&mut random, original, executed);
        let outcome = std::panic::catch_unwind(|| parse_with_options(&bytes, options));
        let result = match outcome {
            Ok(result) => result,
            Err(_) => {
                let path = save_failure(&config, executed, &bytes)?;
                return Err(format!(
                    "panic at case {executed}; saved {}",
                    path.display()
                ));
            }
        };
        if executed % 1024 == 0 {
            let repeated_outcome = std::panic::catch_unwind(|| parse_with_options(&bytes, options));
            if repeated_outcome.as_ref().ok() != Some(&result) {
                let path = save_failure(&config, executed, &bytes)?;
                return Err(format!(
                    "non-deterministic result at case {executed}; saved {}",
                    path.display()
                ));
            }
            repeated += 1;
        }
        if result.is_ok() {
            accepted += 1;
            // Feed valid mutations back into a bounded corpus without displacing
            // the original javac seeds. This is not coverage-guided selection.
            if corpus.len() < MAX_CORPUS {
                corpus.push(bytes);
            } else if initial_count < MAX_CORPUS {
                let index = initial_count + random.below(MAX_CORPUS - initial_count);
                corpus[index] = bytes;
            }
        }
        executed += 1;
        if start.elapsed() >= next_progress {
            println!(
                "progress cases={executed} accepted={accepted} elapsed_seconds={:.1}",
                start.elapsed().as_secs_f64()
            );
            next_progress += Duration::from_secs(15);
        }
    }
    println!(
        "result seed={} cases={executed} accepted={accepted} rejected={} deterministic_rechecks={repeated} corpus={} elapsed_seconds={:.3} failures=0",
        config.seed,
        executed - accepted,
        corpus.len(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match config().and_then(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fuzz_parser: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_collection_accepts_the_last_slot_and_reports_the_first_excess_file() {
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/classes/referencep");
        let mut fixture_paths = Vec::new();
        corpus_files(&directory, &mut fixture_paths).unwrap();
        assert!(!fixture_paths.is_empty());
        let slots = MAX_CORPUS - fixture_paths.len();
        let mut paths = vec![PathBuf::new(); slots];
        corpus_files(&directory, &mut paths).unwrap();
        assert_eq!(paths.len(), MAX_CORPUS);
        assert_eq!(&paths[slots..], fixture_paths);
        let mut paths = vec![PathBuf::new(); slots + 1];
        assert_eq!(
            corpus_files(&directory, &mut paths).unwrap_err(),
            format!("corpus contains more than {MAX_CORPUS} class files")
        );
        assert_eq!(paths.len(), MAX_CORPUS);
    }
}
