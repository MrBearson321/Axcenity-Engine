---
description: Set up Collaborate in about five minutes, start a session, invite people from anywhere over Tailscale, and fix what usually goes wrong.
icon: users
samples: [hello-touched]
---

# Collaborate: set up and work together

Collaborate lets several people edit one project at the same time, each in their own editor. One person **hosts** the session from their editor; the others **join** with an invite link. What you can do once you're in a session is in [Projects](projects.md#collaborate-editing-together); this page is about getting there.

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
5. **Checking this computer** shows this computer's Tailscale name (such as `pauls-pc.tail1234.ts.net`), and that the port Collaborate uses is free. **Continue**.
6. **Ready**. The Output panel says how long setup took (on this computer only; it isn't sent anywhere). The goal is under five minutes.

## Start a session

1. Open your project and choose **File > Start Collaborate Session…**.
2. Name the session (people see the name when they join), choose whether the invite lets someone **Edit** or **Only look**, and how long it works (1, 7 or 30 days). Choose **Start Session**.
3. **Your session is running** shows the invite link. **Copy link** and send it by chat or email. Each link works once; make more from the **Collaborate** panel.

Keep your editor open while others work: the session runs in it. A team that wants a session that doesn't depend on anyone's editor can run the Collaborate server, `axcenity-collab <project folder>`, on an always-on computer.

## Invite someone outside your network

Over Tailscale, the other person's computer has to be able to reach yours:

- **Same Tailscale account** (simplest, for your own computers or a small team that shares one): there's nothing more to do.
- **Their own Tailscale account**: share your computer with them once. In the **Your session is running** window choose **Share in Tailscale** (or open Tailscale's admin console at login.tailscale.com), find this computer under **Machines**, then **…** > **Share**, and send them that share link too. They accept it, and from then on every invite of yours works for them. Sharing is free and they can only reach your computer, nothing else on your network.

## Join a session

1. Make sure Tailscale is running and signed in on your computer, if the host uses it.
2. Open the invite link. On the start screen, choose **Join…** and paste it; in an open editor, choose **File > Join Collaborate Session…** and paste it into the **Collaborate** panel. Where Axcenity was installed with its installer (Windows and Linux), clicking an `axcenity://` link opens the editor with the link filled in. Either way, the Join window says whose session it is before anything connects: join only if you expected the invite.
3. Your editor fetches the session's project into a folder of its own and opens it. The first time takes longest; after that only changes are fetched.

## If it doesn't work

| What you see | What to do |
|---|---|
| **Can't reach … yet. Still trying…** with a `.ts.net` name | Check Tailscale is on and signed in on your computer. If you use your own Tailscale account, ask the host to share their computer with you (above). |
| **Can't reach … yet** with a number address | The host's invite works only on their network. Ask them to set up Tailscale and send a new invite. |
| Windows asks whether to allow Axcenity on networks | Choose **Allow**. Hosting needs it. |
| **this invite was already used** or **has expired** | Ask the host for a new link. |
| Tailscale says **waiting for approval** | Your Tailscale network's admin approves new computers. Ask them, or use your own account. |
| Joined, but you can't change anything | You joined as a viewer. The host can make you an Editor in the **Collaborate** panel. |
| **Can't reach …** after several tries with wrong or old links | The host turns an address away for a minute after ten failed joins in a minute. Wait a minute, then use a fresh link. |

When it still doesn't work, the **Output** panel on both computers says what Collaborate tried. Tailscale's own app shows whether the other computer is online.
