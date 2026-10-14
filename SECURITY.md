# Security policy

## Reporting a vulnerability

Report vulnerabilities privately through
[GitHub's private advisory form](https://github.com/qjsoong/im/security/advisories/new).
Please do not open a public issue first.

Include what you did, what happened, and what you expected. A minimal
reproduction is worth more than a description.

You can expect an acknowledgement within about a week. This is a small project
maintained in spare time, so please be patient with a fix.

## What is in scope

`im` reads text files and talks to the X server. The interesting classes of bug
are therefore:

- **Memory safety.** The workspace forbids `unsafe` code with
  `#![forbid(unsafe_code)]`; a way to reach undefined behaviour without adding
  `unsafe` is a serious finding.
- **Untrusted dictionary or configuration parsing.** `im` is meant to load
  dictionaries from anywhere, including a file you just downloaded. A malformed
  dictionary or `im.toml` must produce an error, never a panic, an unbounded
  allocation, or a hang.
- **Denial of service.** A small file that makes `im` consume unreasonable
  memory or time.
- **The XIM server.** `im-xim` parses messages from arbitrary clients on the
  same X display and writes keyboards events back. Anything a malicious client
  can make it do to another client is in scope.

## What is out of scope

- Reading a dictionary requires reading a file, so a malicious dictionary you
  chose to install is trusted in the same way any other program's data file is.
- `im` does not store credentials, does not use the network, and does not run
  privileged. There is no sandbox to escape.

## Supported versions

The project is pre-1.0 and there are no released versions yet. Fixes land on
`main`.
