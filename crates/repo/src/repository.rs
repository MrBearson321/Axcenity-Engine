//! Linking a project folder to a remote repository (P1 of
//! `docs/research/collaborate-persistent.md`): the brief's
//! `RepositoryManager`.
//!
//! A project folder becomes (or already is) the top of a Git work tree
//! whose remote `origin` is the project's repository. Linking never
//! deletes or overwrites a file:
//!
//! - **An empty remote** gets the folder as its first checkpoint.
//! - **A remote with commits**, linked to a folder with no history of its
//!   own, is *adopted*: the folder's branch is moved onto the remote's
//!   commit without touching the files, so they show as changes against
//!   it. When they differ, nothing is uploaded until the person chooses
//!   ([`upload_my_version`] or [`take_remote_version`], which first puts
//!   the folder's own files aside in a Git stash).
//! - **A folder inside another work tree** (a project in the engine's
//!   `examples/`, say) is refused: committing it would commit to someone
//!   else's repository.
//!
//! Unlinking removes the remote only; the files and the history stay.

use std::path::{Path, PathBuf};

use crate::git::{Author, Change, Git, GitError, Pushed};

/// The remote's name in every linked project.
pub const REMOTE: &str = "origin";

/// Message trailers that mark Axcenity's own commits.
pub const CHECKPOINT_TRAILER: &str = "Axcenity-Checkpoint";

/// What linking needs: the remote and, optionally, the branch to use
/// (else the remote's default, else `main`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteSpec {
    pub url: String,
    pub branch: Option<String>,
    /// Point an existing `origin` that names another URL at this one.
    pub replace_remote: bool,
}

/// Where a folder stands with Git.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Folder {
    /// Not in any work tree.
    NotARepository,
    /// The top of its own work tree.
    Root,
    /// Inside another work tree, whose top is this.
    InsideAnother(PathBuf),
}

/// Why linking didn't happen.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error(
        "this project is inside another Git repository ({0}). Move it to a folder of its own to link it."
    )]
    InsideAnother(PathBuf),
    #[error(
        "this project already uses another repository ({0}). Choose to replace it, or unlink it first."
    )]
    OtherRemote(String),
    #[error(
        "this project already has its own history, unrelated to the repository's. Link it to an empty repository, or make a new one."
    )]
    UnrelatedHistory,
    #[error(transparent)]
    Git(#[from] GitError),
}

/// What linking did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Linked {
    /// The remote was empty: the folder is its first checkpoint.
    Uploaded { commit: String },
    /// The folder now sits on the remote's commit. `differences` are the
    /// folder's files that differ from it (none: already the same).
    Adopted { differences: Vec<Change> },
    /// The folder already had history that the remote shares; sync takes
    /// it from here.
    Related,
}

/// The repository as this computer last saw it (no network).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub remote: Option<String>,
    /// The commit the folder is at.
    pub head: Option<String>,
    /// Commits here the remote hasn't got, and the reverse (as of the
    /// last fetch).
    pub ahead: u32,
    pub behind: u32,
    /// Files changed since the last commit.
    pub changes: usize,
    /// A merge was left half-made (the editor stopped during a sync).
    pub merge_in_progress: bool,
}

/// Where `dir` stands with Git.
pub fn folder_state(git: &Git, dir: &Path) -> Folder {
    match git.toplevel(dir) {
        None => Folder::NotARepository,
        Some(top) if same_folder(&top, dir) => Folder::Root,
        Some(top) => Folder::InsideAnother(top),
    }
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The `.gitignore` a linked project gets when it has none.
pub const GITIGNORE: &str = "\
# Axcenity: per-computer data (cooked cache, Version History, Collaborate's
# journal and recovery files) and temporary files never go in the repository.
.axcenity/
*.tmp
Thumbs.db
desktop.ini
.DS_Store
";

/// Large binary files Git LFS keeps when it's installed.
pub const LFS_PATTERNS: &[&str] = &[
    "*.glb",
    "*.gltf.bin",
    "*.fbx",
    "*.obj",
    "*.blend",
    "*.png",
    "*.jpg",
    "*.jpeg",
    "*.tga",
    "*.bmp",
    "*.hdr",
    "*.exr",
    "*.ktx2",
    "*.dds",
    "*.wav",
    "*.ogg",
    "*.mp3",
    "*.flac",
    "*.mp4",
    "*.webm",
    "*.ttf",
    "*.otf",
    "*.axpkg",
];

/// Writes `.gitignore` (and, with `lfs`, `.gitattributes`) if missing.
/// Returns the files it wrote.
pub fn write_ignore_files(dir: &Path, lfs: bool) -> std::io::Result<Vec<&'static str>> {
    let mut wrote = Vec::new();
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, GITIGNORE)?;
        wrote.push(".gitignore");
    }
    let attributes = dir.join(".gitattributes");
    if lfs && !attributes.exists() {
        let mut text = String::from("# Large binary files go through Git LFS.\n");
        for pattern in LFS_PATTERNS {
            text.push_str(&format!("{pattern} filter=lfs diff=lfs merge=lfs -text\n"));
        }
        std::fs::write(attributes, text)?;
        wrote.push(".gitattributes");
    }
    Ok(wrote)
}

/// Links `dir` to `spec`'s remote; see the module notes. `git` should
/// carry the token when the remote needs one.
pub fn link(
    git: &Git,
    dir: &Path,
    spec: &RemoteSpec,
    author: &Author,
) -> Result<(String, Linked), LinkError> {
    let state = folder_state(git, dir);
    if let Folder::InsideAnother(top) = state {
        return Err(LinkError::InsideAnother(top));
    }
    if state == Folder::Root
        && let Some(url) = git.remote_url(dir, REMOTE)
        && url != spec.url
        && !spec.replace_remote
    {
        return Err(LinkError::OtherRemote(url));
    }
    // Ask the remote first: if it can't be reached, nothing has changed.
    let remote = git.remote_default_branch(None, &spec.url)?;
    let branch = spec
        .branch
        .clone()
        .or(remote.branch.clone())
        .unwrap_or_else(|| "main".to_owned());
    let created = state == Folder::NotARepository;
    if created {
        git.init(dir, &branch)?;
    }
    let lfs = git.has_lfs();
    if lfs {
        let _ = git.run(dir, &["lfs", "install", "--local"]);
    }
    write_ignore_files(dir, lfs).map_err(|e| GitError::Failed(e.to_string()))?;
    let previous = git.remote_url(dir, REMOTE);
    git.set_remote(dir, REMOTE, &spec.url)?;
    let result = link_to_remote(git, dir, remote.empty, &branch, author);
    if result.is_err() {
        if created {
            // The history made a moment ago held nothing yet: the folder
            // goes back to not being a repository.
            let _ = std::fs::remove_dir_all(dir.join(".git"));
        }
        // Leave the folder's remote as it was.
        match &previous {
            Some(url) => {
                let _ = git.set_remote(dir, REMOTE, url);
            }
            None => {
                let _ = git.remove_remote(dir, REMOTE);
            }
        }
    }
    result.map(|linked| (branch, linked))
}

fn link_to_remote(
    git: &Git,
    dir: &Path,
    remote_empty: bool,
    branch: &str,
    author: &Author,
) -> Result<Linked, LinkError> {
    if remote_empty {
        let message = crate::checkpoint_message(
            "First checkpoint",
            "manual",
            &[author.name.as_str()],
            "The project as it was when it was linked to this repository.",
        );
        let commit = match git.commit_all(dir, &message, author)? {
            Some(commit) => commit,
            None => git
                .head(dir)
                .ok_or_else(|| GitError::Failed("nothing to upload".into()))?,
        };
        return match git.push(dir, REMOTE, branch)? {
            Pushed::Done => Ok(Linked::Uploaded { commit }),
            // Someone pushed in between: treat it like a remote with commits.
            Pushed::Behind => link_to_remote(git, dir, false, branch, author),
        };
    }
    git.fetch(dir, REMOTE, branch)?;
    let theirs = format!("{REMOTE}/{branch}");
    match git.head(dir) {
        None => {
            git.reset_keep_files(dir, &theirs)?;
            Ok(Linked::Adopted {
                differences: git.changes(dir)?,
            })
        }
        Some(head) => {
            let related = git
                .run(dir, &["merge-base", &head, &theirs])
                .is_ok_and(|base| !base.trim().is_empty());
            if related {
                Ok(Linked::Related)
            } else {
                Err(LinkError::UnrelatedHistory)
            }
        }
    }
}

/// After an adoption: brings back from the repository each of `paths`
/// the folder doesn't have (a session member's copy, which never holds
/// some files, such as the World's own, before it uploads). Paths the
/// folder has are left alone.
pub fn bring_back_missing(git: &Git, dir: &Path, paths: &[String]) -> Result<(), GitError> {
    let missing: Vec<&str> = paths
        .iter()
        .map(String::as_str)
        .filter(|path| !dir.join(path).exists())
        .collect();
    // A few hundred at a time keeps the command line short everywhere.
    for chunk in missing.chunks(200) {
        let mut args = vec!["checkout", "--quiet", "HEAD", "--"];
        args.extend(chunk);
        git.run(dir, &args)?;
    }
    Ok(())
}

/// After an adoption with differences: the folder's version wins, as a
/// checkpoint on top of the remote's (nothing of the remote's is lost:
/// it's the commit before).
pub fn upload_my_version(
    git: &Git,
    dir: &Path,
    branch: &str,
    author: &Author,
) -> Result<Option<String>, GitError> {
    let message = crate::checkpoint_message(
        "Uploaded this computer's version",
        "manual",
        &[author.name.as_str()],
        "The project folder's files as they were when it was linked.",
    );
    let commit = git.commit_all(dir, &message, author)?;
    match git.push(dir, REMOTE, branch)? {
        Pushed::Done => Ok(commit),
        Pushed::Behind => Err(GitError::Failed(
            "the repository changed meanwhile; sync to bring its changes in first".into(),
        )),
    }
}

/// After an adoption with differences: the remote's version wins. The
/// folder's own files are first put aside in a Git stash ("Axcenity:
/// this computer's version before taking the repository's"), so nothing
/// is lost; `git stash list` shows it.
pub fn take_remote_version(git: &Git, dir: &Path, branch: &str) -> Result<(), GitError> {
    if !git.changes(dir)?.is_empty() {
        git.run(
            dir,
            &[
                "stash",
                "push",
                "--include-untracked",
                "--quiet",
                "--message",
                "Axcenity: this computer's version before taking the repository's",
            ],
        )?;
    }
    let theirs = format!("{REMOTE}/{branch}");
    git.run(dir, &["reset", "--quiet", "--hard", &theirs])?;
    Ok(())
}

/// Unlinks `dir`: removes the remote. The files and history stay.
pub fn unlink(git: &Git, dir: &Path) -> Result<(), GitError> {
    git.remove_remote(dir, REMOTE)
}

/// Where `dir` stands, from what this computer has (no network).
pub fn status(git: &Git, dir: &Path, branch: &str) -> Status {
    let remote = git.remote_url(dir, REMOTE);
    let head = git.head(dir);
    let theirs = format!("{REMOTE}/{branch}");
    let (ahead, behind) = if head.is_some() && git.rev(dir, &theirs).is_some() {
        git.ahead_behind(dir, &theirs).unwrap_or((0, 0))
    } else if head.is_some() {
        // Never uploaded: everything is ahead.
        let count = git
            .run(dir, &["rev-list", "--count", "HEAD"])
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0);
        (count, 0)
    } else {
        (0, 0)
    };
    Status {
        remote,
        head,
        ahead,
        behind,
        changes: git.changes(dir).map(|c| c.len()).unwrap_or(0),
        merge_in_progress: git_dir(git, dir).is_some_and(|d| d.join("MERGE_HEAD").exists()),
    }
}

/// The folder's `.git` directory.
pub fn git_dir(git: &Git, dir: &Path) -> Option<PathBuf> {
    let out = git.run(dir, &["rev-parse", "--absolute-git-dir"]).ok()?;
    Some(PathBuf::from(out.trim()))
}

/// Clones `url` into the new folder `dest` (to open a project nobody has
/// on this computer yet).
pub fn clone_project(
    git: &Git,
    url: &str,
    dest: &Path,
    branch: Option<&str>,
) -> Result<(), GitError> {
    git.clone_repository(url, dest, branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{author, bare_remote, git, temp, write};

    fn spec(url: &str) -> RemoteSpec {
        RemoteSpec {
            url: url.to_owned(),
            branch: None,
            replace_remote: false,
        }
    }

    #[test]
    fn an_empty_remote_gets_the_project_as_its_first_checkpoint() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let project = temp("project");
        write(&project, "project.json", "{}");
        write(&project, "scripts/Door.server.lua", "print('door')");
        write(&project, ".axcenity/cache/x.bin", "cache");
        let (branch, linked) = link(&git, &project, &spec(&url), &author("Paul")).unwrap();
        assert_eq!(branch, "main");
        let Linked::Uploaded { commit } = linked else {
            panic!("{linked:?}")
        };
        // The remote has it, without the per-computer folder.
        let clone = temp("clone");
        let dest = clone.join("p");
        git.clone_repository(&url, &dest, None).unwrap();
        assert!(dest.join("scripts/Door.server.lua").exists());
        assert!(!dest.join(".axcenity").exists());
        assert!(dest.join(".gitignore").exists());
        assert_eq!(git.head(&dest), Some(commit));
        let log = git.log(&dest, "HEAD", 5).unwrap();
        assert_eq!(log[0].subject, "[Collaborate] First checkpoint");
        assert_eq!(log[0].trailer(CHECKPOINT_TRAILER), Some("manual"));
        assert_eq!(log[0].author, "Paul");
        let status = status(&git, &project, "main");
        assert_eq!((status.ahead, status.behind, status.changes), (0, 0, 0));
    }

    #[test]
    fn a_remote_with_commits_is_adopted_without_touching_files() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        // Someone else's copy, uploaded already.
        let theirs = temp("theirs");
        write(&theirs, "project.json", "{\"name\":\"Castle\"}");
        write(&theirs, "scripts/Gate.lua", "gate");
        link(&git, &theirs, &spec(&url), &author("Sam")).unwrap();

        // The same files: adopted, nothing differs.
        let same = temp("same");
        write(&same, "project.json", "{\"name\":\"Castle\"}");
        write(&same, "scripts/Gate.lua", "gate");
        let (_, linked) = link(&git, &same, &spec(&url), &author("Paul")).unwrap();
        assert_eq!(
            linked,
            Linked::Adopted {
                differences: vec![]
            }
        );

        // Other files: adopted, the differences listed, files untouched.
        let other = temp("other");
        write(&other, "project.json", "{\"name\":\"Castle 2\"}");
        let (branch, linked) = link(&git, &other, &spec(&url), &author("Paul")).unwrap();
        let Linked::Adopted { differences } = linked else {
            panic!()
        };
        let mut paths: Vec<_> = differences.iter().map(|c| c.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["project.json", "scripts/Gate.lua"]);
        assert_eq!(
            std::fs::read_to_string(other.join("project.json")).unwrap(),
            "{\"name\":\"Castle 2\"}"
        );
        assert!(!other.join("scripts/Gate.lua").exists());

        // Taking the remote's version keeps ours in a stash.
        take_remote_version(&git, &other, &branch).unwrap();
        assert_eq!(
            std::fs::read_to_string(other.join("project.json")).unwrap(),
            "{\"name\":\"Castle\"}"
        );
        assert!(other.join("scripts/Gate.lua").exists());
        let stashes = git.run(&other, &["stash", "list"]).unwrap();
        assert!(
            stashes.contains("before taking the repository's"),
            "{stashes}"
        );
    }

    #[test]
    fn uploading_my_version_puts_it_on_top_of_the_remotes() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let theirs = temp("theirs");
        write(&theirs, "a.txt", "a");
        link(&git, &theirs, &spec(&url), &author("Sam")).unwrap();
        let mine = temp("mine");
        write(&mine, "b.txt", "b");
        let (branch, _) = link(&git, &mine, &spec(&url), &author("Paul")).unwrap();
        upload_my_version(&git, &mine, &branch, &author("Paul")).unwrap();
        let log = git.log(&mine, "HEAD", 5).unwrap();
        assert_eq!(log.len(), 2, "the remote's commit is still the one before");
        assert_eq!(log[1].author, "Sam");
    }

    #[test]
    fn a_copy_gets_back_only_the_missing_files_it_names() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let theirs = temp("theirs");
        write(&theirs, "world.json", "{\"old\":1}");
        write(&theirs, "levels/Two.json", "two");
        write(&theirs, "scripts/Gone.lua", "gone");
        write(&theirs, "scripts/Kept.lua", "old");
        link(&git, &theirs, &spec(&url), &author("Paul")).unwrap();
        // A session copy: no World files, one script deleted, one changed.
        let copy = temp("copy");
        write(&copy, "scripts/Kept.lua", "new");
        let (_, linked) = link(&git, &copy, &spec(&url), &author("Sam")).unwrap();
        let Linked::Adopted { differences } = linked else {
            panic!("{linked:?}")
        };
        let not_carried: Vec<String> = differences
            .into_iter()
            .map(|c| c.path)
            .filter(|p| !p.starts_with("scripts/"))
            .chain(["scripts/Kept.lua".to_owned()])
            .collect();
        bring_back_missing(&git, &copy, &not_carried).unwrap();
        assert_eq!(
            std::fs::read_to_string(copy.join("world.json")).unwrap(),
            "{\"old\":1}"
        );
        assert!(copy.join("levels/Two.json").exists());
        // What the copy has stays; what it deleted stays deleted.
        assert_eq!(
            std::fs::read_to_string(copy.join("scripts/Kept.lua")).unwrap(),
            "new"
        );
        assert!(!copy.join("scripts/Gone.lua").exists());
    }

    #[test]
    fn a_failed_first_link_leaves_no_history_behind() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let theirs = temp("theirs");
        write(&theirs, "a.txt", "a");
        link(&git, &theirs, &spec(&url), &author("Paul")).unwrap();
        // The remote answers but its branch can't be fetched.
        let project = temp("project");
        write(&project, "b.txt", "b");
        let spec = RemoteSpec {
            branch: Some("missing".into()),
            ..spec(&url)
        };
        assert!(link(&git, &project, &spec, &author("Sam")).is_err());
        assert!(!project.join(".git").exists());
        assert!(project.join("b.txt").exists());
    }

    #[test]
    fn the_remotes_default_branch_is_used() {
        let git = git();
        let remote = temp("remote-trunk");
        git.run(
            &remote,
            &["init", "--quiet", "--bare", "--initial-branch", "trunk"],
        )
        .unwrap();
        let url = format!("file://{}", remote.display());
        let first = temp("first");
        write(&first, "a.txt", "a");
        let mut s = spec(&url);
        s.branch = Some("trunk".into());
        link(&git, &first, &s, &author("Sam")).unwrap();
        let second = temp("second");
        let (branch, _) = link(&git, &second, &spec(&url), &author("Paul")).unwrap();
        assert_eq!(branch, "trunk");
    }

    #[test]
    fn a_project_inside_another_repository_is_refused() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let outer = temp("outer");
        git.init(&outer, "main").unwrap();
        let project = outer.join("examples").join("hello");
        std::fs::create_dir_all(&project).unwrap();
        let error = link(&git, &project, &spec(&url), &author("Paul")).unwrap_err();
        assert!(matches!(error, LinkError::InsideAnother(_)), "{error:?}");
        assert!(!project.join(".git").exists());
        assert!(!project.join(".gitignore").exists());
    }

    #[test]
    fn another_remote_is_kept_unless_replacing_is_chosen() {
        let git = git();
        let (_a, url_a) = bare_remote(&git);
        let (_b, url_b) = bare_remote(&git);
        let project = temp("project");
        write(&project, "a.txt", "a");
        link(&git, &project, &spec(&url_a), &author("Paul")).unwrap();
        assert_eq!(
            link(&git, &project, &spec(&url_b), &author("Paul")).unwrap_err(),
            LinkError::OtherRemote(url_a.clone())
        );
        let mut replace = spec(&url_b);
        replace.replace_remote = true;
        link(&git, &project, &replace, &author("Paul")).unwrap();
        assert_eq!(git.remote_url(&project, REMOTE), Some(url_b));
    }

    #[test]
    fn unrelated_history_is_refused_and_the_remote_left_as_it_was() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let theirs = temp("theirs");
        write(&theirs, "a.txt", "a");
        link(&git, &theirs, &spec(&url), &author("Sam")).unwrap();
        let mine = temp("mine");
        git.init(&mine, "main").unwrap();
        write(&mine, "b.txt", "b");
        git.commit_all(&mine, "mine", &author("Paul")).unwrap();
        assert_eq!(
            link(&git, &mine, &spec(&url), &author("Paul")).unwrap_err(),
            LinkError::UnrelatedHistory
        );
        assert_eq!(git.remote_url(&mine, REMOTE), None);
    }

    #[test]
    fn an_unreachable_remote_changes_nothing() {
        let git = git();
        let project = temp("project");
        write(&project, "a.txt", "a");
        let missing = format!("file://{}", temp("gone").join("nothing").display());
        assert!(link(&git, &project, &spec(&missing), &author("Paul")).is_err());
        assert!(!project.join(".git").exists());
        assert!(!project.join(".gitignore").exists());
    }

    #[test]
    fn unlinking_keeps_files_and_history() {
        let git = git();
        let (_remote, url) = bare_remote(&git);
        let project = temp("project");
        write(&project, "a.txt", "a");
        link(&git, &project, &spec(&url), &author("Paul")).unwrap();
        let head = git.head(&project);
        unlink(&git, &project).unwrap();
        assert_eq!(git.remote_url(&project, REMOTE), None);
        assert_eq!(git.head(&project), head);
        assert!(project.join("a.txt").exists());
        // And it links again.
        let (_, linked) = link(&git, &project, &spec(&url), &author("Paul")).unwrap();
        assert_eq!(linked, Linked::Related);
    }

    #[test]
    fn lfs_attributes_name_binary_assets() {
        let dir = temp("lfs");
        let wrote = write_ignore_files(&dir, true).unwrap();
        assert_eq!(wrote, vec![".gitignore", ".gitattributes"]);
        let text = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
        assert!(text.contains("*.glb filter=lfs diff=lfs merge=lfs -text"));
        // Existing files are left alone.
        std::fs::write(dir.join(".gitignore"), "mine\n").unwrap();
        assert!(write_ignore_files(&dir, true).unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
            "mine\n"
        );
    }
}
