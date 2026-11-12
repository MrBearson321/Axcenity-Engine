---
description: What Collaborate needs a host for, what happens when someone disconnects, where files live, what GitHub and Tailscale each do, what is shared, who gets in, and which outside services are involved.
icon: users
---

# How Collaborate works

This page says, in one place, what Collaborate does behind the scenes. How to set it up and use it is in [Collaborate: set up and work together](collaborate.md).

## Does a session need a host?

Yes. A live session always runs in one place: the **host**. That is the editor of whoever started it, or the Collaborate server (`axcenity-collab`) on an always-on computer. Every change goes to the host, which puts the changes in one order and sends them to everyone. Nobody's editor talks to anyone else's directly.

A **project** doesn't need a host. Between sessions it lives in a folder on each member's computer and, if you connected one, in a Git repository. Anyone who may host can start the next session from their copy.

## When someone disconnects

- **A member**: their editor keeps working offline and tries again by itself. When it's back, its changes are sent. A change that clashes with what someone did meanwhile is listed in **Recovered Changes**, never dropped silently.
- **The host leaves**: **Leave session** hands the session to the next member who may host. Everyone stays connected.
- **The host is lost** (computer off, network gone): members wait about 20 seconds, then the next member on the list hosts from their own copy. Edits made meanwhile are kept.

## Where files live

| What | Where |
|---|---|
| The project | The host's project folder. Each joiner gets a copy in a folder of their own, kept in step while they're in the session. |
| Your identity (name, colour, device key) | The editor's data folder on your computer, never in a project. |
| The members list and invites | The host's data folder, beside the project's record. |
| The session's journal (for recovering from a crash) | Inside the project's `.axcenity` folder on the host, never synced. |
| Checkpoints | The project's Git history, when it has a repository. |

## What GitHub (or any Git host) does

Git keeps the project **between** sessions: checkpoints, history and restores, and a place to pick the project up when nobody is hosting. It is optional, and it never carries live editing: changes in a session travel between editors, not through Git. Your Git sign-in stays with Git on your computer.

## What Tailscale does

Tailscale only **connects computers** in different places, privately, with nothing to set up on a router. It doesn't decide who may join or what they may do; invites, roles and **Who gets in** do. Without Tailscale, Collaborate works on one network (one home, school or office).

## What is shared

Everyone in a session sees the whole project: the world, scripts, assets, levels and settings, plus each other's names, colours, selections, cameras and chat. Nothing outside the project folder is shared, and Collaborate doesn't send anything to Axcenity or anyone else.

## Roles

- **Owner**: everything, including inviting people, changing roles, removing members, restoring checkpoints and ending the session.
- **Editor**: changes the project.
- **Viewer**: sees everything, changes nothing.

The host checks every change against the sender's role, so an editor that's been tampered with can't do more than its role allows.

## Who gets in

To join, someone needs an invite link (each works once, until it expires) or to be a member already. Members are known by their **device key**, which their editor makes and keeps.

The person hosting chooses in **Settings > Collaborate > Who gets in**:

- **Anyone with an invite** (the default): a valid invite lets someone in, and they're a member from then on.
- **Ask before new members join**: members rejoin without asking. Someone using an invite for the first time waits until you choose **Let in** or **Turn away**. You see their name, device key, the role they'd have and where they connect from.
- **Ask every time**: everyone waits for you, members too.

Anyone not answered in two minutes is turned away, and a turned-away invite still works for someone you do let in. To take someone's membership away, **Remove** them in the Collaborate panel: they need a new invite to come back. The Owner's **Audit log** lists who joined and who was turned away.

## Where a host listens

A session you host takes connections only from this computer and over the network others reach you by: Tailscale when you host over it, otherwise this computer's address on its network. **Settings > Collaborate > Listening > Listen on every network** takes connections from every network this computer is on instead. If the chosen address can't be listened on, Axcenity listens on every network and says so in Output.

The Collaborate server (`axcenity-collab`) listens on every network, since it usually runs on a computer kept for it.

## Firewalls

The setup window's **Checking this computer** step asks the firewall about Axcenity, and only reads:

- **Windows**: whether Windows Defender Firewall has a rule allowing or blocking Axcenity. A **Block** rule (Windows adds one when its first prompt is dismissed) wins over any Allow rule, so others can't connect. **Allow through the firewall…** replaces it with one rule letting Axcenity accept connections on its port on private networks. Windows asks you to approve it; nothing is changed without your click.
- **Mac**: whether the Application Firewall blocks Axcenity. Change it in **System Settings > Network > Firewall > Options**.
- **Linux**: whether `ufw` or `firewalld` is on. If one is, the check says the command to open the port.

## Conflicts

Inside a session there are no conflicts: the host orders every change, and script tabs are typed in together like a shared document. Conflicts only happen between a session and the repository, when the same file changed in both while apart. Axcenity keeps both versions and asks which to keep (**Resolve…**).

## Offline

Without a network you keep working. Checkpoints wait on your computer and upload when the network is back.

## Outside services

- **Tailscale**, if you use it: connects the computers. Its app and sign-in are Tailscale's.
- **GitHub** or another Git host, if you connect a repository: keeps checkpoints. Signing in to GitHub uses GitHub's device sign-in page.

Nothing else: there's no Axcenity account or server in between.
