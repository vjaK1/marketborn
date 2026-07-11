//! Marketborn headless CLI: run, replay, hash, diff.

use clap::{Parser, Subcommand};
use sim_cli::{first_divergence, last_common_tick};
use sim_core::{Good, TickError, World, WorldConfig};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "sim-cli",
    version,
    about = "Marketborn headless simulation tools"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a fresh world headlessly and print its hash manifest.
    Run {
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = 365)]
        ticks: u64,
        #[arg(long, default_value_t = 20)]
        population: u32,
        /// Hash cadence in ticks.
        #[arg(long, default_value_t = 50)]
        hash_every: u64,
        /// Write a save file at the end of the run.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Suppress per-cadence manifest lines.
        #[arg(long)]
        quiet: bool,
    },
    /// Rebuild a saved run from config + command log and verify it matches.
    Replay { save: PathBuf },
    /// Print (and verify) the state hash of a save, optionally after
    /// advancing to a later tick.
    Hash {
        save: PathBuf,
        #[arg(long)]
        at: Option<u64>,
    },
    /// Compare two runs' hash manifests; report the first divergent tick
    /// with surrounding commands and events from both saves.
    Diff {
        a: PathBuf,
        b: PathBuf,
        /// Ticks of context to print around the divergence.
        #[arg(long, default_value_t = 3)]
        context: u64,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Run {
            seed,
            ticks,
            population,
            hash_every,
            out,
            quiet,
        } => cmd_run(seed, ticks, population, hash_every, out, quiet),
        Cmd::Replay { save } => cmd_replay(&save),
        Cmd::Hash { save, at } => cmd_hash(&save, at),
        Cmd::Diff { a, b, context } => cmd_diff(&a, &b, context),
    };
    match result {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(2)
        }
    }
}

fn print_summary(world: &World) {
    let s = &world.state;
    let employed = s.agents.values().filter(|a| a.employer.is_some()).count();
    let unemployed = s
        .agents
        .values()
        .filter(|a| a.employer.is_none() && a.owns.is_none())
        .count();
    let hungry = s.agents.values().filter(|a| a.hungry_streak > 0).count();
    println!("tick {}", s.tick);
    println!(
        "  population {} | employed {employed} | unemployed {unemployed} | hungry {hungry}",
        s.agents.len()
    );
    println!("  money total {}", s.total_cash());
    for good in Good::ALL {
        let price = s
            .market
            .last_prices
            .get(&good)
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".into());
        println!("  {good}: last avg price {price}");
    }
    for b in s.businesses.values() {
        println!(
            "  {} [{}] cash {} workers {}/{} price {} stock {}",
            b.name,
            b.kind.label(),
            b.cash,
            b.workers.len(),
            b.target_headcount,
            b.price,
            b.stock(b.sells),
        );
    }
}

fn tick_failure(e: TickError) -> String {
    match e {
        TickError::Invariant(v) => format!("simulation halted\n{v}"),
        other => other.to_string(),
    }
}

fn cmd_run(
    seed: u64,
    ticks: u64,
    population: u32,
    hash_every: u64,
    out: Option<PathBuf>,
    quiet: bool,
) -> Result<ExitCode, String> {
    let config = WorldConfig {
        master_seed: seed,
        population,
        hash_every,
    };
    let mut world = World::from_config(config);
    println!("marketborn run: seed {seed}, {ticks} ticks, population {population}");
    let started = Instant::now();
    for _ in 0..ticks {
        match world.tick() {
            Ok(report) => {
                if let (Some(hash), false) = (&report.hash, quiet) {
                    println!("  tick {:>6}  {hash}", report.tick);
                }
            }
            Err(e) => {
                eprintln!("{}", tick_failure(e));
                return Ok(ExitCode::FAILURE);
            }
        }
    }
    let elapsed = started.elapsed();
    let tps = if elapsed.as_secs_f64() > 0.0 {
        (ticks as f64 / elapsed.as_secs_f64()) as u64
    } else {
        0
    };
    println!(
        "completed {ticks} ticks in {:.2}s ({tps} ticks/s)",
        elapsed.as_secs_f64()
    );
    print_summary(&world);
    if let Some(path) = out {
        sim_persist::save(&world, &path).map_err(|e| e.to_string())?;
        println!("saved to {}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_replay(save: &Path) -> Result<ExitCode, String> {
    let meta = sim_persist::read_meta(save).map_err(|e| e.to_string())?;
    println!(
        "replaying {} (seed {}, tick {})",
        save.display(),
        meta.master_seed,
        meta.tick
    );
    let started = Instant::now();
    let replayed = sim_persist::replay_from_save(save).map_err(|e| e.to_string())?;
    let elapsed = started.elapsed();
    let hash = replayed.state_hash().map_err(|e| e.to_string())?;
    if hash == meta.world_hash {
        println!(
            "replay OK: tick {} hash {hash} matches the save ({:.2}s)",
            meta.tick,
            elapsed.as_secs_f64()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        let stored = sim_persist::read_manifest(save).map_err(|e| e.to_string())?;
        let diverged = first_divergence(&stored, &replayed.journal.manifest);
        eprintln!("replay DIVERGED");
        eprintln!("  saved hash:    {}", meta.world_hash);
        eprintln!("  replayed hash: {hash}");
        match diverged {
            Some(t) => eprintln!("  first divergent manifest tick: {t}"),
            None => eprintln!(
                "  manifests agree on the common range; divergence after the last cadence"
            ),
        }
        Ok(ExitCode::FAILURE)
    }
}

fn cmd_hash(save: &Path, at: Option<u64>) -> Result<ExitCode, String> {
    let meta = sim_persist::read_meta(save).map_err(|e| e.to_string())?;
    let mut world = sim_persist::load(save).map_err(|e| e.to_string())?;
    let recomputed = world.state_hash().map_err(|e| e.to_string())?;
    if recomputed != meta.world_hash {
        eprintln!(
            "warning: stored hash {} does not match recomputed {} — corrupted save?",
            meta.world_hash, recomputed
        );
        return Ok(ExitCode::FAILURE);
    }
    match at {
        None => println!("tick {:>6}  {recomputed}", world.state.tick),
        Some(target) => {
            if target < world.state.tick {
                return Err(format!(
                    "--at {target} is before the save's tick {}; replay the run instead",
                    world.state.tick
                ));
            }
            let delta = target - world.state.tick;
            world.run_ticks(delta).map_err(tick_failure)?;
            let hash = world.state_hash().map_err(|e| e.to_string())?;
            println!("tick {:>6}  {hash}", world.state.tick);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_diff(a: &Path, b: &Path, context: u64) -> Result<ExitCode, String> {
    let ma = sim_persist::read_manifest(a).map_err(|e| e.to_string())?;
    let mb = sim_persist::read_manifest(b).map_err(|e| e.to_string())?;
    match first_divergence(&ma, &mb) {
        None => {
            match last_common_tick(&ma, &mb) {
                Some(t) => println!("runs are identical through tick {t} (no common divergence)"),
                None => println!("runs share no hash cadence ticks; nothing to compare"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Some(tick) => {
            println!("runs diverge at manifest tick {tick}");
            let from = tick.saturating_sub(context);
            let to = tick + context;
            for (label, path) in [("A", a), ("B", b)] {
                println!("--- run {label}: {} ---", path.display());
                let commands =
                    sim_persist::read_commands_range(path, from, to).map_err(|e| e.to_string())?;
                if commands.is_empty() {
                    println!("  commands t{from}..t{to}: none");
                } else {
                    for c in commands {
                        println!(
                            "  command #{} t{} applied={}: {}",
                            c.seq, c.tick, c.applied, c.json
                        );
                    }
                }
                let events =
                    sim_persist::read_events_range(path, from, to).map_err(|e| e.to_string())?;
                if events.is_empty() {
                    println!("  events t{from}..t{to}: none");
                } else {
                    for e in events {
                        println!("  event #{} t{} [{}] {}", e.seq, e.tick, e.kind, e.json);
                    }
                }
            }
            Ok(ExitCode::FAILURE)
        }
    }
}
