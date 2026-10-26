# red-ale

The `red-ale` command: the JSON-RPC server, the MCP server, and a command-line front end to the same methods.

## Role

This crate builds the program people run. It reads the command line and passes everything else to `red-ale-rpc`'s
`Dispatcher`. It has no bookkeeping logic of its own; that is what makes the command line, the server and every other
route into red-ale behave the same.

| Command                                        | What it does                                                                                |
|------------------------------------------------|---------------------------------------------------------------------------------------------|
| `red-ale exec --book <file> <method> [params]` | Opens a book, runs one method or a script of them, saves if anything changed, and closes it |
| `red-ale serve [--book <file>]`                | Keeps a book open and serves the API over HTTP, on `127.0.0.1:7878` by default              |
| `red-ale mcp [--book <file>]`                  | Serves every method as an MCP tool over standard input and output, for AI agents            |
| `red-ale call <method> [params]`               | Sends one method to a running server                                                        |
| `red-ale methods`                              | Lists every method, with its effect and a one-line summary                                  |
| `red-ale schema`                               | Prints the API's JSON Schema                                                                |

Params are one JSON object, or `key=value` pairs for text and `key:=json` pairs for anything else. Results go to
standard output as JSON. Errors go to standard error, as `{"error": …}`, with exit status 2; mistakes in the command
line itself exit with status 1. With `--output <file>`, `exec` and `call` write the `html` of a `report.export` result
to the file and print `{title, path, bytes}` instead.

## Where it fits

This crate puts the others together and is the only one with a `main`. It is also the only crate that uses `anyhow`;
the library crates return typed errors.

```text
red-ale (CLI, HTTP, MCP)  ->  red-ale-rpc  ->  red-ale-service  ->  red-ale-core, red-ale-xml
```

`red-ale mcp` runs `red-ale-mcp`'s server over the same dispatcher.

## Usage

```sh
# Create a book and add an account
red-ale exec --create --book home.gnucash book.info
red-ale exec --book home.gnucash account.create name=Checking type=BANK commodity=USD

# Run several methods; if one fails, nothing is saved
red-ale exec --book home.gnucash --script changes.txt

# Look without changing anything, and without taking the lock
red-ale exec --book home.gnucash --read-only account.list

# Export a report as one self-contained HTML file
red-ale exec --book home.gnucash --read-only report.export report="Balance Sheet" --output balance-sheet.html

# Serve the book, and talk to it from another terminal
red-ale serve --book home.gnucash
red-ale call account.balance account=Checking
red-ale call book.save
```

The [walkthrough in the main README](../../README.md#walkthrough) goes through these step by step.

An MCP client starts `red-ale mcp` itself, for example with
`{"command": "red-ale", "args": ["mcp", "--config", "/home/me/.config/red-ale.json"]}`. Standard output carries only
MCP messages; reports go to standard error. The server stops when the client closes its standard input.

Ctrl-C or SIGTERM stops either server: it closes the book and releases the lock without saving. Unsaved changes stay in
the journal, and `book.replayJournal` can recover them.
