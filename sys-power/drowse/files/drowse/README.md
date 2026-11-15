# drowse

`drowsed`: a system service of systemd. While every session is idle it
does the actions of its stages, one after another: a command (and its
undo when the user comes back), an action of power (suspend, hibernate,
power off). The stages are set for the battery and for AC. It warns
before an action, and any activity cancels it. What it does is in
REQUIREMENTS.md.

## What the system needs

### The idle hint of a graphical session

logind does not measure the idle state of a graphical session; the
session tells it. With no idle hint, the session is never idle, and the
service never acts. With swayidle:

```
exec swayidle idlehint 1
```

Other lines of swayidle (the screen off, the lock) are as you like.

### logind: IdleAction=ignore

Keep `IdleAction=ignore` (the default): the service does the action, not
logind.

A console is idle for logind when no key was pressed for `IdleActionSec`
(30 min by default); the service takes it as logind says. A console left
open so holds the system up for that long. To make it shorter, set
`IdleActionSec=` in `/etc/systemd/logind.conf.d/`; with
`IdleAction=ignore` it changes nothing else.

### UPower: the critical charge (recommended)

```
# /etc/UPower/UPower.conf
CriticalPowerAction=PowerOff
```

The service does not act on a low charge; UPower does. Its action passes
an inhibitor of mode `block-weak` (as that of `drowse-inhibit`), but
not one of mode `block`.

### The action

The default action is `poweroff`: it works on any device. A sleep needs
the kernel and the system set up for it (`CONFIG_SUSPEND`, a place to
resume from for `hibernate`); the uConsole with a CM5 has no suspend.
To sleep, set `Action=suspend` in `/etc/drowse.conf`, in the section of
the power source.

### The commands of the stages: short

A command of a stage (`Action`, `Resume`) has 5 s to run; past it
systemd kills it, and a killed `Action` has no `Resume`. A long task
does not belong in a stage: start it from a unit or a timer of its own.

### A limit: nothing goes through a sleep

systemd stops drowsed before a sleep and starts it after the wake; a
start is a new count from the first stage. So a stage after a sleep
does not come: `[IdleBattery] Action=suspend` and then
`[IdleBattery-1] Action=poweroff` never powers off, the second stage is
never reached.

To sleep and then hibernate after a time, use the action of logind
`Action=suspend-then-hibernate`, with `HibernateDelaySec=` in
`/etc/systemd/sleep.conf`: systemd wakes the system for it. It needs a
place to resume from. A sleep, then a power off, is not done.

### Who sees what holds the system

Anyone reads the state of the service on the bus. What holds the system
up (the sessions, the inhibitors) only the group `wheel` reads: a rule of
the policy of D-Bus of the package. Others see the state with no list.

### Work left running after a logout

drowsed does not count a session that is closing. So what a user leaves
running after the logout (a build, a download, tmux) does not hold the
system up. Hold it by an inhibitor: `drowse-inhibit on`, or
`systemd-inhibit --what=sleep:shutdown <command>`.

### KillUserProcesses=yes (recommended)

```
# /etc/systemd/logind.conf.d/50-kill-user-processes.conf
[Login]
KillUserProcesses=yes
```

Why. With the default `no`, logind does not kill what a user leaves
running at the logout (tmux, a program started with `nohup`, a server).
The session then stays in the state `closing` while such a process lives,
and keeps the last idle hint it got; nobody updates it. This service
leaves `closing` sessions out, so its own idle state stays right. But
logind itself still counts them: `IdleHint` of logind, its `IdleAction`,
and any other program that reads them see the system as "active" for
good. With `yes`, logind kills the processes of a session at its end,
the session goes away, and every reader of the idle state agrees.

What it costs. Processes left in a session after the logout are killed:
a tmux started in that session ends with it. To keep a program after the
logout:

- run it in the user manager, not in the session:
  `systemd-run --user --scope tmux`. It lives while the user has any
  session; with `loginctl enable-linger` it lives always;
- or leave a user out: `KillExcludeUsers=` in `logind.conf`.

With `yes`, an inhibitor of `drowse-inhibit` ends with the session where
it was taken; with `no` it outlives the logout.

## The state in waybar

`drowse-status --waybar --follow` writes one line of JSON at each change,
and as the time left runs; waybar needs no `interval`. A click takes or
lets go the inhibitor of `drowse-inhibit`.

```json
"custom/drowse": {
    "exec": "drowse-status --waybar --follow",
    "return-type": "json",
    "format": "{icon} {}",
    "format-icons": {
        "unknown": "?",
        "announced": "⏰",
        "own": "☕",
        "other": "🔒",
        "session-other": "👤",
        "idle": "🌙",
        "active": ""
    },
    "on-click": "drowse-inhibit toggle"
}
```

With a Nerd Font, the glyphs of Material Design (`unknown` nf-md-help, `announced` nf-md-alarm, `own` nf-md-coffee, `other` nf-md-lock, `session-other` nf-md-account, `idle` nf-md-sleep):

```json
    "format-icons": {
        "unknown": "󰋖",
        "announced": "󰀠",
        "own": "󰅶",
        "other": "󰌾",
        "session-other": "󰀄",
        "idle": "󰒲",
        "active": ""
    },
```

The classes of the module: the state (`active`, `idle`, `announced`,
`unknown`), what holds the system (`own`, `other`, `session-other`,
`session-own`, `none`, `hidden`) and the power (`battery`, `ac`).

```css
#custom-drowse.announced     { color: #f38ba8; animation: blink 1s steps(2) infinite; }
#custom-drowse.own           { color: #a6e3a1; }
#custom-drowse.other         { color: #f9e2af; }
#custom-drowse.session-other { color: #89b4fa; }
#custom-drowse.unknown       { color: #6c7086; }

@keyframes blink { to { opacity: 0.3; } }
```

The texts are in English or Russian, by the locale of the session.
