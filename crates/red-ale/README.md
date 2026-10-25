# red-ale

The `red-ale` command: the JSON-RPC server, and a command-line front end to the same methods.

## Role

This crate builds the program people run. It reads the command line and passes everything else to `red-ale-rpc`'s
`Dispatcher`. It has no bookkeeping logic of its own; that is what makes the command line, the server and every other
route into red-ale behave the same.

| Command                                        | What it does                                                                                |
|------------------------------------------------|---------------------------------------------------------------------------------------------|
| `red-ale exec --book <file> <method> [params]` | Opens a book, runs one method or a script of them, saves if anything changed, and closes it |
| `red-ale serve [--book <file>]`                | Keeps a book open and serves the API over HTTP, on `127.0.0.1:7878` by default              |
| `red-ale call <method> [params]`               | Sends one method to a running server                                                        |
| `red-ale methods`                              | Lists every method, with its effect and a one-line summary                                  |
| `red-ale schema`                               | Prints the API's JSON Schema                                                                |

Params are one JSON object, or `key=value` pairs for text and `key:=json` pairs for anything else. Results go to
standard output as JSON. Errors go to standard error, as `{"error": …}`, with exit status 2; mistakes in the command
line itself exit with status 1.

## Where it fits

This crate puts the others together and is the only one with a `main`. It is also the only crate that uses `anyhow`;
the library crates return typed errors.

```text
red-ale (CLI, HTTP)  ->  red-ale-rpc  ->  red-ale-service  ->  red-ale-core, red-ale-xml
```

The MCP server (v0.2) will be added here as another front end over the same dispatcher.

## Usage

```sh
# Create a book and add an account
red-ale exec --create --book home.gnucash book.info
red-ale exec --book home.gnucash account.create name=Checking type=BANK commodity=USD

# Run several methods; if one fails, nothing is saved
red-ale exec --book home.gnucash --script changes.txt

# Look without changing anything, and without taking the lock
red-ale exec --book home.gnucash --read-only account.list

# Serve the book, and talk to it from another terminal
red-ale serve --book home.gnucash
red-ale call account.balance account=Checking
red-ale call book.save
```

The [walkthrough in the main README](../../README.md#walkthrough) goes through these step by step.

Ctrl-C or SIGTERM stops the server: it closes the book and releases the lock without saving. Unsaved changes stay in
the journal, and `book.replayJournal` can recover them.
