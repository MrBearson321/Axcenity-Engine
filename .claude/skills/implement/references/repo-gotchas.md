# Repository implementation gotchas

Load only the notes relevant to the files or checks being changed:

- Git checks text files out with Windows line endings (`core.autocrlf=true`). Normalize `\r\n` before exact text comparisons.
- On Windows, Git Bash heredocs can damage Rust source containing apostrophes such as `'static`; use the editor's file-editing tools.
- Minimized windows can report a 0×0 size. Never configure a wgpu surface or texture with a zero dimension.
- The workspace forbids `unsafe` code (`unsafe_code = "forbid"`).
