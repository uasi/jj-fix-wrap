# jj-fix-wrap

Adapter for integrating file-based code formatters with `jj fix`.

## Problem

`jj fix` expects formatters to accept file content on stdin and output the formatted result on stdout. However, some code formatters only support reading and writing regular files.

## Solution

jj-fix-wrap wraps file-based formatters to work with `jj fix` by:

1. Reading stdin into a temporary file
2. Executing the formatter with the temp file path
3. Capturing the formatted output (from tool stdout or modified file) and writing it to stdout

## Installation

```bash
cargo build --release
cp target/release/jj-fix-wrap /usr/local/bin
```

## Usage

Invoke `jj-fix-wrap` as your formatter command:

```bash
jj-fix-wrap [options] <tool> <tool-arg>...
```

### Options

- `-i`, `--in-place`: Tool modifies input file in-place
- `-f`, `--file FILE`: Original file path
- `-r`, `--root ROOT`: Workspace root

### Placeholder substitution

Tool arguments support these placeholders:

- `%input`: Path to the temporary file; the filename portion is taken from `--file` if provided, otherwise `input`
- `%file`: Original file path (if provided with `--file`)
- `%root`: Workspace root (if provided with `--root`)
- `%%`: Literal `%`

### Placeholder substitution order

Substitution happens in two stages within your jj configuration:

1. **jj's substitution**: `jj fix` replaces `$file` and `$root` in the entire command before executing it
2. **jj-fix-wrap's substitution**: jj-fix-wrap then replaces `%file`, `%root`, and `%input` in tool arguments only (the arguments after the tool name)

Use `$file` and `$root` for jj-fix-wrap's own arguments (before the tool name), and `%file`, `%root`, and `%input` for the wrapped tool's arguments (after the tool name).

```toml
# Good
command = [
    "jj-fix-wrap", "--root=$root",
    "myformatter", "--work-dir=%root", "%input",
]

# Bad: uses $root in tool arguments
command = [
    "jj-fix-wrap", "--root=$root",
    "myformatter", "--work-dir=$root", "%input",
    #                          ^---- here
]
```

In the bad example, `$root` is substituted by jj before jj-fix-wrap runs. If the resulting path contains characters that look like jj-fix-wrap placeholders (e.g., `%root` or `%input`), they will be substituted again by jj-fix-wrap, causing unexpected behavior.

### Examples

Format stdin with `sed`:

```bash
$ echo 'hello world' | jj-fix-wrap sed 's/world/jj/' %input
hello jj
```

Configure in `.jj/repo/config.toml`:

```toml
[fix.tools.cargo-fmt]
command = [
    "jj-fix-wrap", "--file=$file", "--in-place",
    "cargo", "fmt", "--", "%input",
]
patterns = ["glob:**/*.rs"]
```
