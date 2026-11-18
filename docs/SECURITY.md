# Vimora security notes

## Threat model

The people in the room are **students: the adversary is a user of the student PC and of the same
LAN**. We assume they can run their own programs as an ordinary user, sniff and spoof traffic on the
classroom network, and try to disable or bypass the software. We do **not** claim protection against
someone with administrator rights, physical access to disks, or a malicious teacher.

| Threat | Defence |
|---|---|
| Fake teacher takes over student PCs | PCs pin the teacher's Ed25519 key; every session is signed; privileged commands are signed per command with session nonce + counter (no replay) |
| Man in the middle injects or reads session data | X25519 key exchange signed by the teacher; ChaCha20-Poly1305 records with counters; PCs that once spoke encrypted refuse plain sessions (no downgrade) |
| Oversized / malformed frames before authentication | 64 KiB limit until the peer is authenticated, 5 s handshake timeouts; local IPC limited to 1 MiB |
| Student calls the privileged service directly | Everything dangerous needs a teacher signature; the update path accepts only signed release lists and re-checks a copy held in a folder the student cannot write |
| Replay of an old, signed (vulnerable) release | Installers must be newer than the running build |
| Student stops or removes the software | Guard watcher, protected install folder, uninstall only with a teacher-signed command (`/BYTEACHER`) |
| Stolen teacher identity | `identity.key` is created `0600`; keep it out of backups you share. To rotate: `vimora-student unpair` on the PCs and a new teacher identity |
| Release key loss / theft | Key kept offline; `RELEASE_KEYS` supports several keys so a new one can be added before the old one is retired |

## Known limits (accepted, documented)

* **Trust on first use.** The first teacher that reaches an unpaired PC owns it until it is
  unpaired. Pair PCs on a trusted network.
* **Local IPC is not authenticated** (the agent runs as the student). The service therefore must
  never perform a dangerous action on an unsigned IPC request; the few unsigned requests are
  read-only or only tighten restrictions.
* **Plain sessions with PCs that run versions older than 0.3.63** remain possible until those PCs
  are updated; update every PC, then the "encrypted before" memory makes the downgrade permanent.
* Website blocking relies on the hosts file and browser policies. A student with administrator rights
  can undo it; path rules (`site/path`) are enforced only by browsers that read policies.

## Reporting a vulnerability

Please report privately (GitHub → *Security → Report a vulnerability*) and give a reasonable time to
fix it before publishing. Do not test against computers you do not own.
