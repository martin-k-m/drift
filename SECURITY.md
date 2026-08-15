# Security Policy

## Supported versions

`drift` is not published to a registry yet; it is installed from this
repository. When it is published it will be as `drift-tabular`, so a crate named
`drift` or `drift-diff` is not this project and is not covered by this policy.
Fixes land on `main`; there are no long-lived maintenance branches.

## Reporting a vulnerability

Please report suspected vulnerabilities privately rather than in a public
issue. Use GitHub's [private vulnerability reporting](https://github.com/martin-k-m/drift/security/advisories/new)
for this repository, or email martinkmuskov@gmail.com.

Include the two input files and the command that reproduce the problem. You can
expect an acknowledgement within a few days.

## Scope

`drift` reads two local files and writes a diff to standard output. It makes no
network requests and executes no code from its input. The dependency tree is
empty, so the supply-chain surface is the standard library and the toolchain
alone.

The most likely class of issue is a crafted CSV that causes excessive memory or
an unhandled panic rather than a controlled error. The hand-written parser is
the code to scrutinise for that; such reports are in scope and welcome.
