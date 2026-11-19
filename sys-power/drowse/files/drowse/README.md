# drowse

`drowsed`: a system service of systemd. While every session is idle it
enters its sections, one after another: a command (and its undo when the
user comes back), an action of power (suspend, hibernate, power off).
The chains are set for the battery and for AC, with the lid open and
closed. It warns before an action, and any activity cancels it. What it
does is in REQUIREMENTS.md; this file says what the system needs.

## What the system needs

### The idle hint of a graphical session

logind does not measure the idle state of a graphical session; the
session tells it. With no idle hint, the session is never idle, and the
service never acts. With swayidle:

```
exec swayidle idlehint 1
```

Other lines of swayidle (the screen off, the lock) are as you like.

### The login screen

A login screen (a session of class `greeter`) counts, as for logind. If
it sets no idle hint, it is never idle, and the system stays up while it
is shown. Run swayidle with `idlehint` in its sway, or leave `greeter`
out of `sessions.classes` in `/etc/drowse.yaml`: then with nobody logged
in the system is idle.

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

With no chains in `/etc/drowse.yaml` the service does nothing. The file
as the package gives it stays in `/usr/share/drowse/drowse.yaml`, to
compare with or to take again. The file of the package sets, on battery, the
power save at 10 min of idle and `poweroff` at 50 min; on AC nothing.

The power save is the profile of TuneD `cm5-powersave`, for the BCM2712
(a Raspberry Pi 5 or CM5):

- `powersave`;
- the CPU at 800 MHz at most;
- USB autosuspend on every device but those of `KEEP` in
  `usb-autosuspend.sh`: `434b:5543`, the keyboard of the uConsole, and
  `1eaf:*`, its bootloader. The hubs these hang on, up to the root hub,
  are set to never suspend. The script writes the values before back
  when the profile ends;
- the dirty pages written once a minute (`vm.dirty_writeback_centisecs`,
  `vm.dirty_expire_centisecs`): a power cut or a crash loses up to a
  minute of writes;
- runtime PM of every PCI device (`power/control=auto`); a device
  suspends only if its driver allows it, and nvme and RP1 do not;
- printing (`cups`) and the timers of maintenance (`fstrim.timer`,
  `systemd-tmpfiles-clean.timer`) stopped; they start again when the
  profile ends.

A deeper step of the power save is the profile
`cm5-powersave-no-radio`: `cm5-powersave`, and every radio off by its
soft block of rfkill (`rfkill-off.sh`: Wi-Fi, Bluetooth, a modem). A
radio that was off before stays off when the profile ends. Set it in the
next section of `main.battery`, after the one for `cm5-powersave`:

```yaml
main:
  battery:
    - after: 10m
      enter: /usr/libexec/drowse/drowsed-tuned enter cm5-powersave
      leave: /usr/libexec/drowse/drowsed-tuned leave
      hint: power save
    - after: 10m
      enter: /usr/libexec/drowse/drowsed-tuned enter cm5-powersave-no-radio
      leave: /usr/libexec/drowse/drowsed-tuned leave
      hint: radio off
    - after: 28m
      countdown: 2m
      action: poweroff
```

TuneD writes every value before back when the profile ends. The package
gives both profiles as examples (`examples/tuned/`, installed with the
documentation). If `tuned-adm list` does not show them, copy them to the
profiles of TuneD:

```
cp -r /usr/share/doc/drowse-*/examples/tuned/cm5-powersave \
    /usr/share/doc/drowse-*/examples/tuned/cm5-powersave-no-radio \
    /etc/tuned/profiles/
```

The power save needs:

- tuned, and the profile. With no tuned or no profile the power save
  fails, and the `poweroff` still comes.
- `arm_freq_min=800` or lower in `config.txt`. cpufreq keeps the limit
  inside the range of the firmware, and the default lowest of the CM5 is
  1500 MHz.

`drowsed-tuned enter <profile>` keeps the profile of TuneD before and
sets that of the section; `drowsed-tuned leave` gives the one before
back, the mode `auto` as `auto`. If the profile was changed while the
section was entered (by the user, by tuned-ppd), it stays, and the
journal of the unit says so. The profile before is given back as it
was: if the power source changed meanwhile, tuned-ppd does not map it
again.

Another way, with tuned-ppd: `enter: tuned-adm profile cm5-powersave`,
`leave: systemctl try-restart tuned-ppd.service`. At its start tuned-ppd
sets the profile the user chose, for the power source now. The restart
must end within the time of a `leave`.

`poweroff` works on any device. A sleep needs
the kernel and the system set up for it (`CONFIG_SUSPEND`, a place to
resume from for `hibernate`); the uConsole with a CM5 has no suspend.
To sleep, set `action: suspend` in the last section of the power chain
in `/etc/drowse.yaml`.

### The lid

With `lid:` in `/etc/drowse.yaml`, the lid closed picks the chains of
`lid`, the lid open those of `main`. With no `lid:` the lid is not
looked at. A chain of `lid` not set is empty.

When the lid or the power source changes the chain, and the first
section of the new chain runs the same `enter` and `leave` as the
section entered, that section carries over: nothing runs, and the next
section of the new chain counts from that moment. So the power save
stays on when the lid is closed, and the power off comes 10 min later:

```yaml
sections:
  - &powersave
    after: 10m
    enter: /usr/libexec/drowse/drowsed-tuned enter cm5-powersave
    leave: /usr/libexec/drowse/drowsed-tuned leave
    hint: power save

main:
  battery:
    - *powersave
    - {after: 38m, countdown: 2m, action: poweroff}
lid:
  battery:
    - {<<: *powersave, after: 0}
    - {after: 10m, countdown: never, action: poweroff}
```

Any other change of the chain is activity: the section entered is left,
and the count starts anew by the new chain.

logind acts on the lid itself, and drowsed does not hold it. Keep logind
from it in one of two ways:

- `HandleLidSwitch=ignore` and `HandleLidSwitchExternalPower=ignore` in
  `/etc/systemd/logind.conf.d/`;
- or an inhibitor of `handle-lid-switch` taken in the graphical session
  (`systemd-inhibit --what=handle-lid-switch sleep infinity` started by
  sway). logind takes it only from a process of an active session.

In a bag the keys get pressed. A key is activity, so the chain of `lid`
may never reach its end.

### The commands of the sections: short

A command section (`enter`, `leave`) has 15 s to run; past it systemd
kills it, and a killed `enter` has no `leave`. A long task
does not belong in a section: start it from a unit or a timer of its own.

### A limit: nothing goes through a sleep

systemd stops drowsed before a sleep and starts it after the wake; a
start is a new count from the first section. So a section after a sleep
does not come: a chain with `action: suspend` and then
`action: poweroff` never powers off, the second section is never
reached.

To sleep and then hibernate after a time, use the action of logind
`action: suspend-then-hibernate`, with `HibernateDelaySec=` in
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
`drowse-inhibit -- <command>` while the command runs.

### drowse-inhibit and polkit

`drowse-inhibit` holds the sections with a command by `idle`, by
default. `--power` holds the actions of logind (`sleep:shutdown`);
`--all` holds both. Its inhibitors are of mode `block-weak`, which
swayidle does not read: the screen still goes off and locks. In a local
session polkit gives them with no password. From ssh polkit asks the
password of an admin. On a terminal the tool runs `pkttyagent` (of
polkit), and the password is typed there. With no `pkttyagent`, or with
no terminal, the inhibitor from ssh is refused. Root is never asked: the
tool then starts no agent.

### A long command: emerge

Hold the system up while the command runs:

```
drowse-inhibit -- emerge -uDN @world
```

Use `--power` to hold only logind actions, or `--all` for both. Separate
holds let a due action run in the gap between commands. One shell command
can hold the whole sequence:

```
drowse-inhibit --all -- sh -c 'first; second'
```

Use `on`, `off` and `toggle` with the same flags. The default is
`--idle`: a build keeps the CPU at full speed, and the screen still goes
off.

The inhibitor ends with the command; its exit code is that of the
command. As root (`sudo drowse-inhibit -- emerge ...`) the inhibitor is
of uid 0; the module of waybar shows it as its own too.

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
and as the time left runs; waybar needs no `interval`. The module shows
the icon only; the time left and what holds the system are in the
tooltip. A click runs `drowse-inhibit cycle`: none, then `--power`, then
`--all`, then none. It does not touch an inhibitor of
`drowse-inhibit -- <command>`, which the tooltip shows as
`until <command>`.

With Waybar support enabled, the package puts the module in
`/etc/xdg/waybar/drowse.jsonc` and its colours in
`/etc/xdg/waybar/drowse.css` (the files of `waybar/`, with the icons
and the classes). In the config of waybar:

```jsonc
"include": ["/etc/xdg/waybar/drowse.jsonc"],
"modules-right": ["custom/drowse", "clock"],
```

and in its `style.css`:

```css
@import "/etc/xdg/waybar/drowse.css";
```

With a Nerd Font, the glyphs of Material Design in place of the icons
of the module:

```json
    "format-icons": {
        "unknown": "󰋖",
        "announced": "󰀠",
        "held": "󰌾",
        "own-all": "󰅶",
        "own-power": "󰌾",
        "own-idle": "󰒓",
        "other": "󰌾",
        "session-other": "󰀄",
        "idle": "󰒲",
        "session-own": "󰍹",
        "active": ""
    },
```

The texts are in English or Russian, by the locale of the session.
