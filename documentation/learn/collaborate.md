---
description: Set up Collaborate in about five minutes, start a session, invite people from anywhere over Tailscale, keep the project in a Git repository when everyone leaves, and fix what usually goes wrong.
icon: users
samples: [hello-touched]
---

# Collaborate: set up and work together

Collaborate lets several people edit one project at the same time, each in their own editor. One person **hosts** the session from their editor; the others **join** with an invite link. What you can do once you're in a session is in [Projects](projects.md#collaborate-editing-together); this page is about getting there. What needs a host, where files live, who gets in and which outside services are involved are in [How Collaborate works](how-collaborate-works.md). The start screen's **Team Projects** page lists the projects you collaborate on.

> **Experimental.** Collaborate is new. Keep backups of a shared project (**File > Back Up Project**).

## What you need

- Axcenity on every computer, the same version.
- **Tailscale**, a free app that connects your computers privately wherever they are, with nothing to set up on your router. You only need it to work with people outside your own network (another home, school or office). Everyone in a session needs it when the host uses it.
- A Google, Microsoft, GitHub or Apple account to sign in to Tailscale. Tailscale has no password of its own.

Tailscale only connects the computers. Who may join your session and change your project is decided by your invite links and roles, never by Tailscale.

## Set up (once per computer)

Choose **File > Set Up Collaborate…** (or just **Start Collaborate Session…**: the first time, setup comes first). The setup window walks you through each step and moves on by itself when a step is done:

1. **Set up Collaborate** explains what happens. Choose **Set up Tailscale**, or **Same network only** if everyone is on your network.
2. **Install Tailscale**: **Download Tailscale** opens its download page for your system. Run the installer (it may ask for your computer's password), then come back to the editor.
3. **Open Tailscale** appears if it's installed but not running: open the Tailscale app. Its icon appears next to the clock (Windows) or in the menu bar (Mac).
4. **Sign in to Tailscale**: **Sign in** opens Tailscale's sign-in page in your browser. When it says you're connected, come back.
5. **Checking this computer** shows this computer's Tailscale name (such as `pauls-pc.tail1234.ts.net`), that the port Collaborate uses is free, and what the firewall says. If Windows' firewall blocks Axcenity, **Allow through the firewall…** fixes it (Windows asks you to approve). **Continue**.
6. **Ready**. The Output panel says how long setup took (on this computer only; it isn't sent anywhere). The goal is under five minutes.

## Start a session

1. Open your project and choose **File > Start Collaborate Session…**.
2. Name the session (people see the name when they join), choose whether the invite lets someone **Edit** or **Only look**, and how long it works (1, 7 or 30 days). Choose **Start Session**.
3. **Your session is running** shows the invite link. **Copy link** and send it by chat or email. Each link works once; make more from the **Collaborate** panel.
4. If **Settings > Collaborate > Who gets in** asks about joiners, a **Someone wants to join** window shows each one's name, device key, role and address: **Let in** or **Turn away**.

Keep your editor open while others work: the session runs in it. A team that wants a session that doesn't depend on anyone's editor can run the Collaborate server, `axcenity-collab <project folder>`, on an always-on computer.

## Invite someone outside your network

Over Tailscale, the other person's computer has to be able to reach yours:

- **Same Tailscale account** (simplest, for your own computers or a small team that shares one): there's nothing more to do.
- **Their own Tailscale account**: share your computer with them once. In the **Your session is running** window choose **Share in Tailscale** (or open Tailscale's admin console at login.tailscale.com), find this computer under **Machines**, then **…** > **Share**, and send them that share link too. They accept it, and from then on every invite of yours works for them. Sharing is free and they can only reach your computer, nothing else on your network.

## Join a session

1. Make sure Tailscale is running and signed in on your computer, if the host uses it.
2. Open the invite link. On the start screen, choose **Join…** and paste it; in an open editor, choose **File > Join Collaborate Session…** and paste it into the **Collaborate** panel. Where Axcenity was installed with its installer (Windows and Linux), clicking an `axcenity://` link opens the editor with the link filled in. Either way, the Join window says whose session it is before anything connects: join only if you expected the invite.
3. Your editor fetches the session's project into a folder of its own and opens it. The first time takes longest; after that only changes are fetched.

## Keep the project online

A session ends when everyone leaves, but the project doesn't have to go with it. Connect it to a Git repository (GitHub or any other Git host) and it waits there for whoever opens it next. This is optional: without a repository, the project lives in the host's folder as before.

1. Open the **Collaborate** panel. At the top, the **Project** section shows the project's name, its Owner and who is hosting.
2. Under **Repository**, choose **Connect a repository…**. Sign in to GitHub (the window shows a code to type at github.com/login/device), then pick an existing repository, create a new one (private unless you change it), or paste the address of any other Git repository.
3. If the repository already has files, nothing uploads until you choose **Upload my version** or **Use the repository's version**. Your files go to Git's stash first, so neither choice loses anything.

From then on Axcenity makes **checkpoints** by itself: after saves (at most every ten minutes), when someone leaves a session, when hosting passes to someone else, and when the session ends. Each one uploads straight away. The chip beside the session chip in the title bar says when the project isn't **Synced**; **Sync now** uploads at once, and **History…** lists every checkpoint with **Restore…**. In a session only the host's computer uploads. Your sign-in is kept by Git on your computer, never in the project.

## When the host leaves

- **Leave session** on the host passes the session to the next person who may host. Everyone stays connected and keeps working; the Output panel says who hosts now.
- **End for everyone** ends the session for all. With a repository, the last state is uploaded first.
- If the host's computer goes off or loses its network, members wait about 20 seconds in case it comes back. Then the next person on the list hosts from their own copy, and edits made in the meantime are kept.

Who hosts next is shown in the Project section while you host. The Owner can name a **Backup host** to go first. If you'd rather your computer never hosts, turn off **Let others host from my computer** (also in **Settings > Collaborate > Hosting**). Whoever takes over must let Axcenity through their firewall on port 47810.

When the new host's copy came from the session, Axcenity connects it to the project's repository the first time it hosts, so checkpoints keep uploading from there.

## Pick the project up later

- **Rejoin**: if you were in a session of this project, the Project section offers **Rejoin** while someone hosts it. You don't need a new invite.
- **From the repository**: clone the repository (or download it from GitHub) and open the folder in Axcenity. It's the same project, even with nobody left in a session. If you were in its sessions before and may host, your computer kept its members list, so starting a session from it lets everyone who was in back in without an invite. Otherwise the session starts with you as its host, and you invite people as usual.

## Plan the work on the Team Board

The **Team Board** (**Window > Team Board**, or **Open** on the Team Board line in the Collaborate panel) is the project's shared list of tasks. Type a task at the top and press Enter; click it to fill it in: who it's assigned to, its status, priority, due date, labels, a description, a checklist, links to objects or scripts, and comments. **Board** shows a column per status, and you can drag a card to another column. **Mine** shows only what's assigned to you.

The board also tracks bugs. Choose **Bug** in the menu beside the add field, or right-click an object in the Explorer and choose **Report a Bug…**. When a script error is in the Output, **Report the last error** files it with the message and a link to the script line. A bug has a severity (Blocker to Trivial), steps to reproduce, what should happen and what happens, the version it was found in and what fixed it. **Bugs** shows only bugs; the board counts the open ones and the blockers.

The board is saved in the project's `team` folder, so everyone in a session sees each change within a few seconds, and the project's repository keeps it with everything else. When someone assigns you a task, a message says so. Viewers can read the board but not change it. If two people change the same task at once, both changes are kept: comments and checklist steps from each, and the other fields from whoever changed it last.

## When the same file changed in two places

If a file changed both on your computer and in the repository (for example two people worked on the same scene while apart), Axcenity overwrites neither. Syncing stops, and the repository's version is put beside yours with `.remote-` and a short code in its name, such as `world.remote-1a2b3c4.json`. Those copies are never uploaded.

Choose **Resolve…** in the Project section. For each file, choose **Keep mine**, **Keep theirs** or **Keep both** (yours stays in place and theirs is kept, for everyone, as the copy). **Compare** shows text files side by side. **Resolve and upload** finishes once every file has a choice. Whatever you choose, both versions stay in the History. The Console's `conflicts` and `resolve` do the same.

## Working offline

Without a network, keep working. Checkpoints are kept on your computer, the chip says **Offline**, and Axcenity tries again by itself. When the network is back they upload; if someone uploaded in the meantime, their changes are merged in first, and anything changed on both sides becomes a choice as above.

## If it doesn't work

| What you see | What to do |
|---|---|
| **Can't reach … yet. Still trying…** with a `.ts.net` name | Check Tailscale is on and signed in on your computer. If you use your own Tailscale account, ask the host to share their computer with you (above). |
| **Can't reach … yet** with a number address | The host's invite works only on their network. Ask them to set up Tailscale and send a new invite. |
| Windows asks whether to allow Axcenity on networks | Choose **Allow**. Hosting needs it. If you dismissed it, **File > Set Up Collaborate…** checks the firewall and offers **Allow through the firewall…**. |
| Joining says **Signing in…** for a long time | The host may be asked to let you in. Ask them to choose **Let in**. |
| **The host didn't let you in** | The host turned the join away, or didn't answer in two minutes. Ask them, then join again with the same link. |
| **this invite was already used** or **has expired** | Ask the host for a new link. |
| Tailscale says **waiting for approval** | Your Tailscale network's admin approves new computers. Ask them, or use your own account. |
| Joined, but you can't change anything | You joined as a viewer. The host can make you an Editor in the **Collaborate** panel. |
| The chip says **Error** or **Offline** | Open the Collaborate panel: the Project section says why. Signed out of GitHub: **Manage…** signs you in again. |
| **No one is hosting**, and **Rejoin** does nothing | Nobody who may host has the project open in a session. Start one from your copy, or open it from the repository. |
| **Can't reach …** after several tries with wrong or old links | The host turns an address away for a minute after ten failed joins in a minute. Wait a minute, then use a fresh link. |

When it still doesn't work, the **Output** panel on both computers says what Collaborate tried. Tailscale's own app shows whether the other computer is online.
