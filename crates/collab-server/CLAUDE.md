# `collab-server`

- Collaborate's self-hosted server (data folder `Collaborate Server`, taking over `Team Create Server` once) (TC7 of `docs/research/team-create.md`): `axcenity-collab <project>` hosts a project folder the way the hosting editor does (`collab::hosted`), so members can't tell the two apart. `lib.rs` is the `Server` (start, save every 30 s, recover a journal, stop) and its admin commands; `main.rs` reads them from the terminal.
- Members, keys and the certificate go in the server's data folder, never the project folder. Stopping with `quit` ends the session cleanly; anything else is a crash the next start recovers.
- Tests (`tests.rs`) join over real sockets on 127.0.0.1.
