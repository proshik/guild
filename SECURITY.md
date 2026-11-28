# Security

Guild runs agents and terminals on your Mac, keeps local sockets for its
command line and its agents' hooks, and reads and writes your repositories.
A weakness in any of that can matter more than an ordinary bug, so please
report it privately first.

## Supported versions

Guild is released as nightlies from `main`. Only the **latest nightly** on
the [Releases page](https://github.com/proshik/guild/releases) receives
fixes; a report against an older build is checked against the latest one
first.

## Reporting a vulnerability

Use GitHub's private reporting: open the repository's
[**Security** tab](https://github.com/proshik/guild/security) and choose
**Report a vulnerability**. Please do not open a public issue, pull request
or discussion for it.

Include what you can of:

- the Guild version — **Guild ▸ About Guild** shows it with the build's
  commit;
- your macOS version, and the agent CLI involved and its version, if any;
- what an attacker needs (local access, a crafted repository, a malicious
  agent output, another process on the same Mac) and what they gain;
- steps to reproduce, and the part of the log that shows it
  (`~/Library/Logs/Guild/guild.log`, also **Help ▸ Show Log**). The log can
  hold paths, branch names and command lines; remove anything private
  before you attach it.

## What to expect

The answer comes in the advisory itself: whether the report is confirmed,
and what happens next. A confirmed problem is fixed in a nightly, and the
advisory is published with credit to you unless you ask otherwise. Please
give the fix the chance to ship before you disclose the problem.
