# Security Policy

rCommander is a file manager that people run on their own machines with their files, their SSH
keys and server passwords, and a built-in MCP server that lets an AI agent act on their behalf. A
vulnerability here is somebody's files or somebody's server, so reports are welcome and taken
seriously.

## Reporting a vulnerability

**Report it privately on GitHub** at
`https://github.com/degoya/rCommander/security/advisories/new` (the repository's **Security** tab,
then **Report a vulnerability**). A private vulnerability report is visible only to the
maintainers, which makes it the right channel for something that should not be public yet.

Please do not open a public issue, and please do not post a working exploit anywhere public,
until a fixed release exists.

Useful in a report, roughly in order of how much they help:

- What an attacker gains, and what they need first - a crafted archive or file name, a hostile
  server, a plugin package, a local process talking to the MCP server, a prompt reaching the
  agent.
- The version (Settings, page *About*) and the Windows version.
- The smallest reproduction you have.
- Whether the affected surface was reachable from outside the machine.

You will get an acknowledgement within a week. This is a small project and there is no on-call
rotation, so please read that as a realistic figure rather than a service level.

## What is in scope

Everything shipped from this repository: the program and its MCP server, the vault and the stored
connections, the SSH, SFTP and FTP/FTPS clients and their trust in host keys and certificates, the
archive and import readers, the terminal, the plugin host and its trust model, and the release
archives.

Some things that look like findings are documented behaviour, and a report about them is
answered with a link rather than a fix:

- **The person at the keyboard can run any program and open any file.** The boundaries being
  defended are between the program and what comes from outside - servers, archives, imported
  files, plugins, MCP clients - not between the person and their own machine.
- **An MCP client with the token can do what the MCP policy grants.** The policy (Settings,
  page *MCP server*) decides which tool classes run, ask or are refused; no MCP tool can unlock
  the vault, change the policy or make a trust decision. A path around that is a finding, and a
  serious one.
- **There is no master password until a secret is stored.** The vault is created with the first
  password, key or passphrase saved; before that, nothing secret is kept.

## Supported versions

Security fixes go into the most recent release. There are no long-term-support branches and no
backports to older lines.

| Version | Security fixes |
| --- | --- |
| Latest release | Yes |
| Anything older | No - upgrade |

## What the project already does

Stated so a report can be aimed at what is *not* covered, rather than at what is:

- Stored secrets are encrypted per field with XChaCha20-Poly1305 under a key derived from the
  master password with Argon2id; the vault locks after an idle time and, when set, with the
  Windows session.
- The MCP server listens on `127.0.0.1` only, requires a token, checks the Host and Origin
  headers, gates every tool by its class and masks every stored secret in its answers.
- SSH host keys and FTPS certificates are trusted only by a person, in the window; a changed key
  is refused until then.
- Names from archives, servers and imports are checked before they become paths, so nothing is
  written outside its target folder.
- Plugins are WebAssembly components without file system, network or WASI access, verified
  against signing keys, limited by fuel, memory and time.

## Credit

Reporters are credited in the changelog entry for the fix unless they ask not to be. There is no
bug bounty.
