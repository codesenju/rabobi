//! rabobi — a thin command-line harness over the same engine the
//! desktop app uses. Intended for the Linux loopback integration tests
//! (scripts/) and for scripted/headless operation.
//!
//! Subcommands:
//!   list-drives
//!   scan-images  --dir <dir>
//!   clone        --source <dev> --dest <dev> [--overwrite-table] [--rescue]
//!   backup       --source <dev> --dest-dir <dir> [--compression gzip|zstd|uncompressed] [--rescue]
//!   restore      --image <dir> --dest <dev> [--overwrite-table] [--rescue]

use rescueclone_desktop_lib::backup::run_backup;
use rescueclone_desktop_lib::clone::{run_clone, CancelToken};
use rescueclone_desktop_lib::drives::list_drives;
use rescueclone_desktop_lib::model::{
    BackupRequest, CloneProgressEvent, CloneRequest, RestoreRequest,
};
use rescueclone_desktop_lib::restore::{run_restore, scan_images};
use std::collections::HashMap;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage_and_exit();
    }
    let cmd = args[1].as_str();
    let opts = parse_opts(&args[2..]);

    match cmd {
        "list-drives" => match list_drives() {
            Ok(drives) => {
                for d in &drives {
                    println!(
                        "{}\t{}\t{} bytes\t{}\t{} partitions",
                        d.key,
                        d.model,
                        d.size_bytes,
                        d.partition_table.clone().unwrap_or_else(|| "none".into()),
                        d.partitions.len()
                    );
                    for p in &d.partitions {
                        println!(
                            "  {}\t{}\t{} bytes",
                            p.key,
                            p.filesystem.clone().unwrap_or_else(|| "raw".into()),
                            p.size_bytes
                        );
                    }
                }
            }
            Err(e) => fail(&format!("{e}")),
        },

        "scan-images" => {
            let dir = req(&opts, "dir");
            match scan_images(&dir) {
                Ok(images) => {
                    if images.is_empty() {
                        println!("(no images found)");
                    }
                    for img in &images {
                        println!(
                            "{}\tdisk={}\t{} bytes\t{}\t{} partitions\tnotes={}",
                            img.path,
                            img.disk,
                            img.size_bytes,
                            img.last_modified,
                            img.partitions.len(),
                            img.notes
                        );
                        for p in &img.partitions {
                            println!(
                                "  {}\t{}\t{}\t{} bytes",
                                p.key,
                                p.filesystem.clone().unwrap_or_else(|| "raw".into()),
                                p.compression,
                                p.size_bytes
                            );
                        }
                    }
                }
                Err(e) => fail(&format!("{e}")),
            }
        }

        "clone" => {
            let dest = req(&opts, "dest");
            let request = CloneRequest {
                source_drive_key: req(&opts, "source"),
                destination_drive_key: dest.clone(),
                overwrite_partition_table: flag(&opts, "overwrite-table"),
                rescue: flag(&opts, "rescue"),
                post_action: "nothing".into(),
                mapping: vec![],
                // CLI is non-interactive: auto-confirm the destination the user
                // explicitly named on the command line.
                confirm_device: Some(opts.get("confirm").cloned().unwrap_or(dest)),
                verify: flag(&opts, "verify"),
            };
            run(|t, e| run_clone(&request, t, e));
        }

        "backup" => {
            let request = BackupRequest {
                source_drive_key: req(&opts, "source"),
                partitions: opts
                    .get("partitions")
                    .map(|s| s.split(',').map(|x| x.to_string()).collect())
                    .unwrap_or_default(),
                dest_dir: req(&opts, "dest-dir"),
                compression: opts.get("compression").cloned().unwrap_or_else(|| "zstd".into()),
                compression_level: opts.get("level").and_then(|s| s.parse().ok()),
                rescue: flag(&opts, "rescue"),
                notes: opts.get("notes").cloned().unwrap_or_default(),
                verify: flag(&opts, "verify"),
            };
            run(|t, e| run_backup(&request, t, e));
        }

        "restore" => {
            let dest = req(&opts, "dest");
            let request = RestoreRequest {
                image_dir: req(&opts, "image"),
                destination_drive_key: dest.clone(),
                overwrite_partition_table: flag(&opts, "overwrite-table"),
                rescue: flag(&opts, "rescue"),
                mapping: vec![],
                confirm_device: Some(opts.get("confirm").cloned().unwrap_or(dest)),
                verify: flag(&opts, "verify"),
            };
            run(|t, e| run_restore(&request, t, e));
        }

        _ => usage_and_exit(),
    }
}

/// Run an engine, streaming progress to stderr and printing the summary.
fn run<Feng>(engine: Feng)
where
    Feng: FnOnce(
        &CancelToken,
        &mut dyn FnMut(CloneProgressEvent),
    ) -> Result<rescueclone_desktop_lib::model::CloneSummary, rescueclone_desktop_lib::model::CloneError>,
{
    let token = CancelToken::default();
    let mut last = -1i64;
    let mut emit = |ev: CloneProgressEvent| {
        let pct = (ev.overall_fraction * 100.0) as i64;
        if pct != last {
            last = pct;
            eprint!("\r[{pct:3}%] {}                ", ev.message);
            use std::io::Write;
            let _ = std::io::stderr().flush();
        }
        if let Some(log) = ev.log_line {
            eprintln!("\n  $ {log}");
        }
    };
    match engine(&token, &mut emit) {
        Ok(summary) => {
            eprintln!();
            println!("{}", summary.title);
            for line in &summary.lines {
                println!("  {line}");
            }
            for w in &summary.warnings {
                println!("  ! {w}");
            }
            if let Some(log) = &summary.log_path {
                println!("log: {log}");
            }
            println!("({}s)", summary.duration_seconds);
            if summary.status != "completed" {
                exit(1);
            }
        }
        Err(e) => fail(&format!("{e}")),
    }
}

fn parse_opts(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(key) = a.strip_prefix("--") {
            // boolean flag if next is another --opt or absent
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                map.insert(key.to_string(), args[i + 1].clone());
                i += 2;
            } else {
                map.insert(key.to_string(), "true".to_string());
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    map
}

fn req(opts: &HashMap<String, String>, key: &str) -> String {
    match opts.get(key) {
        Some(v) if v != "true" => v.clone(),
        _ => fail(&format!("missing required --{key}")),
    }
}

fn flag(opts: &HashMap<String, String>, key: &str) -> bool {
    opts.get(key).map(|v| v == "true" || v == "1").unwrap_or(false)
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(2);
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage:\n  rabobi list-drives\n  rabobi scan-images --dir <dir>\n  rabobi clone --source <dev> --dest <dev> [--overwrite-table] [--rescue] [--verify]\n  rabobi backup --source <dev> --dest-dir <dir> [--compression zstd|gzip|uncompressed] [--rescue] [--verify]\n  rabobi restore --image <dir> --dest <dev> [--overwrite-table] [--rescue] [--verify]"
    );
    exit(2);
}
