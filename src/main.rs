// SPDX-License-Identifier: 0BSD

#![deny(clippy::undocumented_unsafe_blocks)]

use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, exit};

use getopts::Options;
use getopts::ParsingStyle;
use tempfile::tempdir;

struct Config {
    in_place: bool,
    file: Option<PathBuf>,
    root: Option<PathBuf>,
    tool: OsString,
    args: Vec<OsString>,
}

impl Config {
    fn file_basename(&self) -> Option<&OsStr> {
        let file = self.file.as_ref()?;
        file.file_name()
    }
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();

    let config = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            exit(1);
        }
    };

    if let Err(e) = run(config) {
        eprintln!("error: {e}");
        exit(1);
    }
}

fn parse_args(args: &[OsString]) -> Result<Config, String> {
    let Some(program) = args.first() else {
        return Err("no program name".to_string());
    };
    let program = program.to_string_lossy();

    let mut opts = Options::new();
    opts.parsing_style(ParsingStyle::StopAtFirstFree);
    opts.optflag("i", "in-place", "Tool modifies input file in-place");
    opts.optopt("f", "file", "Original file path", "FILE");
    opts.optopt("r", "root", "Workspace root", "ROOT");
    opts.optflag("h", "help", "Show this help message");

    let matches = opts.parse(&args[1..]).map_err(|e| e.to_string())?;

    if matches.opt_present("h") {
        let brief = format!("Usage: {program} [options] <tool> <tool-arg>...");
        print!("{}", opts.usage(&brief));
        println!();
        println!("Examples:");
        println!("    $ echo 'hello world' | {program} sed s/world/jj/ %input");
        println!("    hello jj");
        println!();
        println!(r#"    # .jj/repo/config.toml"#);
        println!(r#"    [fix.tools.cargo-fmt]"#);
        println!(r#"    command = ["#);
        println!(r#"        "{program}", "--file=$file", "--in-place","#);
        println!(r#"        "cargo", "fmt", "--", "%input","#);
        println!(r#"    ]"#);
        exit(0);
    }

    if matches.free.is_empty() {
        return Err("tool name required".to_string());
    }

    let num_opts_consumed = args.len() - matches.free.len();

    Ok(Config {
        in_place: matches.opt_present("i"),
        file: matches.opt_str("f").map(PathBuf::from),
        root: matches.opt_str("r").map(PathBuf::from),
        tool: args[num_opts_consumed].clone(),
        args: args[num_opts_consumed + 1..].to_vec(),
    })
}

fn run(config: Config) -> Result<(), String> {
    let mut stdin_content = String::new();

    io::stdin()
        .read_to_string(&mut stdin_content)
        .map_err(|e| format!("failed to read stdin: {e}"))?;

    let work_dir = tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let input_name = config.file_basename().unwrap_or(OsStr::new("input"));
    let input_path = work_dir.path().join(input_name);

    std::fs::write(&input_path, &stdin_content)
        .map_err(|e| format!("failed to write input file: {e}"))?;

    let tool_args = expand_args(&config, &input_path);

    let output = Command::new(&config.tool)
        .args(&tool_args)
        .output()
        .map_err(|e| format!("failed to execute tool: {e}"))?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(1);
        return Err(format!("tool exited with code {code}"));
    }

    write_output(&output, &input_path, config.in_place)?;

    Ok(())
}

fn write_output(output: &Output, input_path: &Path, in_place: bool) -> Result<(), String> {
    let mut stdout = io::stdout();

    if in_place {
        let content = std::fs::read_to_string(input_path)
            .map_err(|e| format!("failed to read modified file: {e}"))?;
        stdout
            .write_all(content.as_bytes())
            .map_err(|e| format!("failed to write output: {e}"))?;
    } else {
        stdout
            .write_all(&output.stdout)
            .map_err(|e| format!("failed to write output: {e}"))?;
    }

    stdout
        .flush()
        .map_err(|e| format!("failed to flush output: {e}"))
}

fn expand_args(config: &Config, input_path: &Path) -> Vec<OsString> {
    const ESCAPED_PERCENT: &str = "\x00";

    config
        .args
        .iter()
        .map(|arg| {
            let result = arg.clone();

            let result = replace_os_str(result, "%%", OsStr::new(ESCAPED_PERCENT));

            let result = replace_os_str(result, "%input", input_path.as_os_str());

            let result = if let Some(f) = &config.file {
                replace_os_str(result, "%file", f.as_os_str())
            } else {
                result
            };

            let result = if let Some(r) = &config.root {
                replace_os_str(result, "%root", r.as_os_str())
            } else {
                result
            };

            replace_os_str(result, ESCAPED_PERCENT, OsStr::new("%"))
        })
        .collect()
}

fn replace_os_str(haystack: OsString, needle: &str, replacement: &OsStr) -> OsString {
    if needle.is_empty() {
        return haystack;
    }

    let haystack = haystack.as_encoded_bytes();
    let needle = needle.as_bytes();

    let positions = {
        let mut positions = Vec::new();
        let mut i = 0;

        while i <= haystack.len().saturating_sub(needle.len()) {
            if haystack[i..].starts_with(needle) {
                positions.push(i);
                i += needle.len();
            } else {
                i += 1;
            }
        }

        positions
    };

    let mut result = OsString::new();
    let mut last_pos = 0;

    for &pos in &positions {
        // SAFETY: The sub-slice `haystack[last_pos..pos]` is originated from the original
        // OsString's encoded bytes, split only at boundaries defined by the needle which is a
        // valid non-empty UTF-8 substring. This preserves the validity of the encoded bytes.
        result.push(unsafe { OsStr::from_encoded_bytes_unchecked(&haystack[last_pos..pos]) });
        result.push(replacement);
        last_pos = pos + needle.len();
    }

    // SAFETY: Ditto for the remaining sub-slice `haystack[last_pos..]`.
    result.push(unsafe { OsStr::from_encoded_bytes_unchecked(&haystack[last_pos..]) });

    result
}
