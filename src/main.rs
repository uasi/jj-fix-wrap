// SPDX-License-Identifier: 0BSD

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Output, exit};

use getopts::Options;
use getopts::ParsingStyle;
use tempfile::tempdir;

struct Config {
    in_place: bool,
    file: Option<String>,
    root: Option<String>,
    tool: String,
    args: Vec<String>,
}

impl Config {
    fn file_basename(&self) -> Option<&str> {
        let file = self.file.as_ref()?;
        let name = Path::new(file).file_name()?;
        name.to_str()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

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

fn parse_args(args: &[String]) -> Result<Config, String> {
    let Some(program) = args.first() else {
        return Err("no program name".to_string());
    };

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

    Ok(Config {
        in_place: matches.opt_present("i"),
        file: matches.opt_str("f").map(|s| s.to_string()),
        root: matches.opt_str("r").map(|s| s.to_string()),
        tool: matches.free[0].clone(),
        args: matches.free[1..].into(),
    })
}

fn run(config: Config) -> Result<(), String> {
    let mut stdin_content = String::new();

    io::stdin()
        .read_to_string(&mut stdin_content)
        .map_err(|e| format!("failed to read stdin: {e}"))?;

    let work_dir = tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let input_name = config.file_basename().unwrap_or("input");
    let input_path = work_dir
        .path()
        .join(input_name)
        .to_string_lossy()
        .into_owned();

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

fn write_output(output: &Output, input_path: &str, in_place: bool) -> Result<(), String> {
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

fn expand_args(config: &Config, input_path: &str) -> Vec<String> {
    const ESCAPED_PERCENT: &str = "\x00";

    config
        .args
        .iter()
        .map(|arg| {
            let mut result = arg.clone();

            result = result.replace("%%", ESCAPED_PERCENT);

            result = result.replace("%input", input_path);
            if let Some(f) = &config.file {
                result = result.replace("%file", f);
            }
            if let Some(r) = &config.root {
                result = result.replace("%root", r);
            }

            result = result.replace(ESCAPED_PERCENT, "%");

            result
        })
        .collect()
}
