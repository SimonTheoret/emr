# Storage specification

The structure follows [arc42](https://arc42.org). Sections 7 and 9 are omitted. Section 4 records the decisions.

## 1. Introduction and goals

### 1.1 Requirements overview

An exchange is one recorded request and its response. Its `id` is the value of the request's `x-erm-match` header. Storage saves the `id` with the exchange.

ERM runs in one of two modes. Each mode uses one storage trait:

| Mode | Trait | Operation | Behavior |
|---|---|---|---|
| Recording | `Recorder` | `save(exchange) -> Result<()>` | Queues the exchange for the filesystem storage and returns. Replaces any earlier exchange with the same `id`. |
| Replaying | `Replayer` | `find(id) -> Option<&Exchange>` | Returns the exchange with this `id` from the in-memory storage, or `None` if there is none. |

When replaying, ERM loads the in-memory storage from the filesystem storage at startup.

### 1.2 Quality goals

| Priority | Goal | Meaning |
|---|---|---|
| 1 | Correctness | `find` returns exactly what `save` stored, with the same bytes, frame order, repeated headers, and delay before each body frame. `find` never returns a request without its response. |
| 2 | Encapsulation | The rest of ERM uses only the storage traits. It never sees file names or the file format. |

## 2. Architecture constraints

- Storage is written in Rust.
- Storage runs on Linux, macOS, and Windows.
- The filesystem storage does not depend on a file format. Each Cargo feature compiles in one format, and ERM's configuration selects the format that the filesystem storage reads and writes.
- File names are valid and distinct on Linux, macOS, and Windows.
- All exchanges fit in memory.
- When recording, the directory is on a local, writable filesystem.
- All storage code lives in one Rust module. The module exposes only the `Recorder` and `Replayer` traits, one constructor for each, and the types in their signatures.

## 3. Context and scope

| Partner | Interaction with storage |
|---|---|
| Request handler, Recording mode | Proxies the request to the upstream, then calls `Recorder::save` with the complete exchange. |
| Request handler, Replaying mode | Calls `Replayer::find` with the request's `id`. |
| ERM startup | Reads the configuration (directory, format, and mode) and calls the constructor for the selected mode. |
| Filesystem | Holds one file per exchange in the configured directory. |

## 4. Solution strategy

| Problem | Approach |
|---|---|
| A read and a write of one `id` race. | Two modes. Recording only writes, and Replaying only reads. A mode that does both would need a per-`id` lock held across the upstream call. |
| Concurrent writes to the filesystem. | One writer thread receives exchanges over a channel and writes them in order. |
| A crash leaves a partial file. | Write to a temporary file, then rename it into place. |
| `find` must not touch the disk. | At startup, Replaying loads every exchange into an in-memory map keyed by `id`. |
| File names must be valid on all platforms. | The `Id` type validates the `x-erm-match` value, and the file name is `<id>.<ext>`. Rewriting invalid values instead could map two `id`s to one file. |
| The format must be replaceable. | Cargo features compile in formats, and the configuration selects one. |
| The rest of ERM must not depend on storage internals. | One module exposes only the traits, their constructors, and the types in their signatures. |
| Bad state must not go unnoticed. | ERM exits with an error on a failed write or a corrupted file. |

## 5. Building block view

| Block | Responsibility |
|---|---|
| `Id`, `Exchange` | Public types. `Id::parse` validates an `x-erm-match` value. |
| `Recorder` trait, filesystem recorder | Sends each exchange over a channel to one writer thread, which writes the temporary file and renames it. |
| `Replayer` trait, in-memory replayer | A `HashMap<Id, Exchange>`, filled once at startup. Nothing writes to it afterward, so it needs no lock. |
| Loader | Reads the directory, parses every file, checks that each file name matches its `id`, and reports every corrupted file. |
| `Format`, private trait | Serializes and deserializes an exchange and names its extension. Each Cargo feature adds one implementation. |

## 6. Runtime view

### Startup

| Mode | Steps |
|---|---|
| Recording | Creates the directory if it is missing, then starts the writer thread. |
| Replaying | Loads every `<id>.<ext>` file at the top level of the directory into the in-memory storage. If the directory is missing or holds no exchanges, logs a warning and starts with no exchanges. |

Both modes ignore subdirectories, `.tmp` files, and files with another extension.

### Saving an exchange

1. The request handler calls `save`, which queues the exchange and returns.
2. The writer thread takes the next exchange, writes `<id>.<ext>.tmp`, and renames it to `<id>.<ext>`.
3. If the write fails, ERM exits. See [Filesystem errors while recording](#filesystem-errors-while-recording).

### Shutdown

On Ctrl-C, or SIGTERM on Linux and macOS, ERM stops accepting connections, finishes in-flight requests, writes every queued exchange, and exits.

## 8. Cross-cutting concepts

### File names

The filesystem storage writes each exchange to `<id>.<ext>`, where `<ext>` is the selected format's extension. Saving an `id` again replaces its file.

The `Id` type accepts a value only if it:

- contains only lowercase ASCII letters, digits, `-`, and `_`,
- has 1 to 200 characters,
- is not a name that Windows reserves: `con`, `prn`, `aux`, `nul`, `com0` to `com9`, or `lpt0` to `lpt9`.

These rules give each `id` its own file name, valid on all three platforms.

### Atomic writes

The filesystem storage writes each exchange to `<id>.<ext>.tmp` in the same directory, then renames it to `<id>.<ext>`. A crash during the write leaves the previous `<id>.<ext>` intact. The loader ignores `.tmp` files.

The filesystem storage does not call `fsync`. A power loss or an OS crash can still lose a recent write or leave its file empty.

### Filesystem errors while recording

If writing an exchange fails, ERM prints an error that names the file and the cause, then exits with a non-zero status. ERM never keeps proxying without recording.

### Corrupted files while replaying

A file is corrupted if it does not parse in the selected format, or if its name does not match the `id` it contains. At startup, ERM checks every file, prints each corrupted file with the cause, then exits with a non-zero status before it accepts connections.
