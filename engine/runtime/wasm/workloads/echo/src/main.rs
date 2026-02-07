/// Echo workload: reads stdin, writes it verbatim to stdout.
/// Also reads any files in /inputs/ and writes their contents to stdout.
use std::io::{self, Read, Write};

fn main() {
    // Echo stdin to stdout
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).expect("failed to read stdin");
    io::stdout().write_all(&input).expect("failed to write stdout");

    // Also read any files from /inputs/ if accessible
    if let Ok(entries) = std::fs::read_dir("/inputs") {
        let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        paths.sort_by_key(|e| e.file_name());
        for entry in paths {
            if let Ok(contents) = std::fs::read(entry.path()) {
                io::stdout().write_all(&contents).expect("failed to write file contents");
            }
        }
    }
}
