# Requirements

While the system is idle, the service enters the sections of its chains,
one after another: a command and its undo, an action of power. It takes
the idle state, the inhibitors and the lid from logind, and the power
source from UPower.

## Terms

- **Idle**: every counted session of logind is idle. Inhibitors do not
  count as activity. Sessions
  of all kinds count alike: a graphical one, a console, ssh. A session
  in the state `closing` is left out: after a logout nobody updates its
  idle state (notes/session-end.md). A session of a class not in
  `sessions.classes` is left out (see "Settings"). The service works
  this out itself from the sessions (`ListSessionsEx()`, the state of
  each), not from `IdleHint` of the Manager, which counts `closing`
  sessions too.
- **The start of the idle state**: the latest start of idle of the
  sessions counted. It starts no sooner than the last of these moments,
  each counted as activity:
  - a session that was not idle leaves the count (`closing`, removed),
    when the service sees it;
  - the chain in use changes (the power source, the lid), unless the
    section entered carries over (see "The chain in use changes");
  - the service starts (see "Sleep").
- **The move to `closing`** sends no signal on D-Bus. The service learns
  of it by the sd-login monitor (`sd_login_monitor_new("session")`),
  which fires when logind writes the state of a session again: at once
  when the leader exits, at most 20 s after the logout
  (notes/session-end.md).
- **The signals of logind** the service reads logind again for:
  `SessionNew`, `SessionRemoved`, and `PropertiesChanged` of the Manager
  with `IdleHint`, `IdleSinceHint`, `IdleSinceHintMonotonic`,
  `BlockInhibited`, `BlockWeakInhibited` or `LidClosed`. It takes no
  other signal of logind: none of a session, a user or a seat. logind
  sends `IdleHint` of the Manager with that of any session
  (notes/logind-interface.md).
- **Inhibitor of screen and system**: a Wayland idle inhibitor, set by
  hand or by a program (a video player, a browser). The session does not
  become idle, and no action comes. In sway it holds only while its
  surface is shown: not while the screen is locked, nor while the output
  is off, nor while its window is hidden (notes/sway-inhibitors.md). This
  is meant so: it holds while the screen is used. To keep the system up
  with the screen off or locked, the inhibitor of system is for that.
- **Inhibitor of system**: an inhibitor of logind, `block` or
  `block-weak`, of `idle`, `sleep` or `shutdown`, taken by hand or by a
  program. It holds the entry into a section of its kind (see "An
  inhibitor of system"). It holds with the screen locked or off. An
  inhibitor of `idle` also stops swayidle (no screen off, no lock); one of
  `sleep` and `shutdown` does not (notes/sway-inhibitors.md).

## Cases

| Case | The action |
|---|---|
| Work in any session (sway, console, ssh) | no |
| All sessions idle, on battery | by `main.battery` |
| All sessions idle, on AC | by `main.ac` |
| All sessions idle, the lid closed, `lid:` in the file | by `lid.battery` or `lid.ac` |
| An ssh connection is open (with a terminal or not) | no |
| Inhibitor of screen and system | no |
| Inhibitor of system | the entry of its kind waits |
| The charger is put in or taken out, the lid closed or opened, while idle | the same section carries over, or a new count from that moment |
| The charge is low or critical | by UPower |

An ssh session is never idle for logind, with a terminal or not
(notes/session-types.md). So it keeps the system up while it is open.

## The power source

- The service takes it from UPower: `OnBattery` and its changes
  (notes/power-source.md).
- With no UPower (upowerd not running, no answer), the service takes the
  power to be AC. So nothing goes down by the time on battery.

## The lid

- The service takes it from logind: `LidClosed` of the Manager, read at
  the start and again on each signal of logind it takes (`PropertiesChanged`
  is sent on its change, notes/logind-interface.md). With no lid it is
  `false`.
- With `lid:` in the file, the lid closed picks the group `lid`, the
  lid open the group `main`. With no `lid:`, the lid is not looked at:
  `main` always, and a change of the lid is nothing.
- A chain of `lid` not set (`lid.ac`) is empty; it is not taken from
  `main`.
- logind acts on the lid itself (`HandleLidSwitch=`); the service does
  not hold it. README.md says how to keep logind from it.

## The part in the session

Each graphical session tells logind when it becomes idle and when it is
active again (`SetIdleHint`). Only this is asked of it. With swayidle it
is the line `idlehint <seconds>` (notes/swayidle.md).

Everything else of the idle program of the session is for the user: the
times of the screen off and of the lock, on battery or on AC.

## The action

The chain in use is picked by the lid and the power source (see "The
lid"). The service enters its sections one at a time while the system
stays idle. A section has `after`,
`countdown`, and either a command (`enter`, `leave`), a logind action
(`action`), or neither. The chains are in `/etc/drowse.yaml` (see
"Settings").

- Entry comes after `after` and `countdown` from the start of idle for
  the first section, or from the entry into the previous section.
- A command section runs `enter` on entry and `leave` when it is left.
  With neither, entry does nothing. A failed or killed `enter` has no
  `leave`; the next section still counts from that entry.
- A section is left before the next one is entered, and on activity.
  A failed or killed `leave` is a warning; the next section is entered
  all the same.
- One section at most is entered at a time, and one job runs at a time.
  A command that started runs to its end.
- The last section stays entered while the system is idle; it is left on
  activity.

1. The system becomes idle. The service publishes the next action and its
   time.
2. When `after` passes, the service announces: "action X in
   `countdown`". Entry comes `countdown` after the announce really
   starts, so a late announce still runs in full. With `countdown: never`
   there is no announce. The section before stays entered during it.
3. Activity before entry cancels it: the entered section is left, and
   the count starts anew at the first section.
4. Else the section before is left and the service waits for its `leave`.
   The state stays `announced` meanwhile. The service then reads logind
   again. With no activity, it enters the section; the next one counts
   its `after` from this entry. Activity in that wait cancels entry.
5. After the last section the service does nothing until the system is
   active and idle again.

### An inhibitor of system

- An inhibitor holds entry by section kind: `idle` holds command
  sections; `sleep` holds sleep actions; `shutdown` holds poweroff and
  halt. `block` and `block-weak` hold alike; `delay` does not.
- A hold is not activity. It does not reset the count or leave an
  entered section. A due entry waits and enters when the hold ends. If
  the hold ends before the entry time, entry stays on time.
- An inhibitor taken during an announce does not cancel it. Activity
  still cancels the announce and starts at the first section.
- If logind refuses an action as inhibited, the entry waits for the
  matching inhibitor to end.
- The journal gets one `held` line per wait and one `released` line when
  a logged hold ends.

### The chain in use changes

The chain in use is picked by the group (`main`, `lid`) and the power
source. When the power source or the lid changes it:

- If a section is entered (its unit started, its `enter` run or
  running), and the first section of the new chain has the same `enter`
  and `leave`, word for word, at least one of them set: that section
  becomes the first section of the new chain. Neither the `leave` of
  the one nor the `enter` of the other runs. The next section of the new
  chain counts its `after` from that moment. `after` of the first
  section is not waited for, and no hold holds it: nothing runs. The
  unit that runs stays, named for the section where it started; its
  `leave` is the same.
- Else the change is activity: a running announce is cancelled, the
  section entered is left, and the count starts anew from that moment
  by the new chain. The power that goes and comes back again and again
  (a bad cable) has no rule of its own: each change is the same.
- A change of the power or the lid that leaves the chain as it is (the
  lid with no `lid:`, the power from AC to not known) is nothing.
- The journal gets one line: the power source or the lid, and that the
  times moved, or which section carried over to which.

### The action itself

- `enter` is a command. `action` is an action of logind. `hint` is the
  name the bus and panel show; by default they show the `enter` command
  or the logind action as written.
- A command section has no default command. Without `enter` and `leave`,
  it is entered with nothing done.
- Just before the action, the service reads the sessions and the
  inhibitors from logind again (`ListSessionsEx()`, `BlockInhibited`,
  `BlockWeakInhibited`), not from what it keeps from the signals.
- A command entry is held by `idle`; logind actions are held by the kind
  logind checks: `sleep` for sleep actions and `shutdown` for poweroff
  and halt. A program that holds only `sleep` does not hold poweroff.
- An action of logind is one of `poweroff`, `halt`, `suspend`,
  `hibernate`, `hybrid-sleep`, `suspend-then-hibernate`, `sleep`. The
  service calls it through logind (`*WithFlags`,
  `SD_LOGIND_ROOT_CHECK_INHIBITORS`). It has no `leave` command.
- A section has one name, as it stands in the file: the group, the power
  source, the number from 1 (`main.battery.1`, `lid.ac.2`). The journal,
  the unit and the errors of the settings give this name.
- A command runs in `drowsed-stage@.service` (the instance is the name
  of the section), which runs `drowsed stage enter <instance>`;
  `leave` runs when the unit stops, as `drowsed stage leave <instance>`.
  `drowsed stage` reads the command from `/etc/drowse.yaml`, splits it
  as a shell does (no shell
  runs: no `|`, `&&`, `$VAR`) and runs it by `exec`; the path is looked
  up in `PATH` of the unit.
- The service does not wait for the end of a command. Its exit code
  gives nothing; its output is in the journal of the unit.
- A command has 15 s to run (`TimeoutStartSec=15s`), its `leave` 15 s
  (`TimeoutStopSec=15s`). Past it systemd kills it: a warning in the
  journal; a killed `enter` has no `leave`. A long task does not
  belong in a section.
- The unit gives the command a directory of its own,
  `/run/drowse/<section>` (`RuntimeDirectory=drowse/%i`, in
  `$RUNTIME_DIRECTORY`): made empty at its start, kept while the section
  is entered, deleted after its stop. `enter` keeps there what `leave`
  needs.
- A section with no `enter` but a `leave` has a unit too: its start runs
  nothing, its stop runs `leave`.
- `leave` runs whenever an entered section is left: before the next one
  is entered, on activity (a change of the chain that does not carry
  the section over is activity), a stop of the service, a sleep. The service keeps a queue of
  jobs for each unit: a start, then a stop, one at a time, the next when
  the one before has ended (`JobRemoved` of systemd). So a stop never
  cuts a start that runs. An entry waits until no job of any unit runs.
  A stop that waits for a command that still runs is a warning in the
  journal.
- When logind refuses the action, or the unit of a command cannot be
  started, the service writes the refusal to the journal, with the
  answer (for a sleep, the error `SleepVerbNotSupported` and its text:
  disabled by the settings, not in the kernel, no place to resume from).
  It is a failed entry, not a stop and not a state: the next section
  comes as if the action had run.
- At its stop (a sleep, `systemctl stop`, a restart) the service waits
  for the commands that run, stops every unit of a section that ran (so
  their `leave` runs, 20 s at most), then exits: at most 30 s
  (`TimeoutStopSec=30s`). At its start it stops every unit of a section
  that is left from before.

### Sleep

- systemd stops the service before any sleep and starts it after the
  wake; the service has no code of its own for a sleep and does not
  follow `PrepareForSleep`.
- Before a sleep: `drowsed.service` has `Conflicts=sleep.target` and
  `Before=sleep.target`. The units of the sections that ran stop with it
(`PartOf=`), so each `leave` runs before the sleep.
- After the wake: `drowsed-resume.service`, wanted by `suspend.target`,
  `hibernate.target`, `hybrid-sleep.target`,
  `suspend-then-hibernate.target` and ordered after them, starts the
  service again. A start is a new count.
- A sleep that does not go through the targets of systemd (a write to
  `/sys/power/state`) is not seen.
- A limit: nothing goes through a sleep. While the system sleeps the
  service is not there; after the wake it starts from the first section. So
  a section after a sleep (`suspend`, then `poweroff` some hours later) does
  not come: only systemd can wake the system for it. For a sleep, then
  hibernation, systemd has `suspend-then-hibernate`; a sleep, then a
  power off, the project does not do.

## The state on the bus

The service publishes its state on the system bus, with
`PropertiesChanged` on each change. A program that shows it (the tool of
the user, a panel) reads it there and does not work it out again.

| Property | Value |
|---|---|
| `State` | `s`: `active` (a counted session is in use), `idle` (the time runs), `announced` (the warning runs or waits for a held entry), `held` (a held entry with no announce) |
| `ScheduledAction` | `(st)`: the hint of the next section and when it comes; time is 0 when a due entry waits, or `("", 0)` when none comes |
| `HeldBySessions` | `a(sussss)`: the sessions that hold the system up; on the object `/io/github/noktoborus/Drowse/HeldBy` |
| `NextHeld` | `b`: whether an inhibitor holds the entry into the next section now; false when the system is in use or no section comes |
| `HeldByInhibitors` | `a(ssssuu)`: the inhibitors that hold the entry into the next section now, in the form of `ListInhibitors()` of logind: what, who, why, mode, uid, pid; on the object `/io/github/noktoborus/Drowse/HeldBy` |
| `PowerSource` | `s`: `battery`, `ac`, or `unknown` when UPower does not answer (the power is then taken as AC) |
| `Lid` | `s`: `open` or `closed`, as logind says; `open` with no lid |

- A time is in microseconds of `CLOCK_MONOTONIC`.
- A session in `HeldBySessions` has the fields of logind as they are:
  id, uid, user, type, tty, remote host. The service gives no kind of
  its own.
- In `active`, `HeldBySessions` has the sessions that are not idle; it
  is empty otherwise. In `idle`, `announced` and `held`,
  `HeldByInhibitors` has the inhibitors of the kind of the next section
  (see "An inhibitor of system"), while its time runs and while its
  entry waits; it is empty in `active`. Only the modes `block` and
  `block-weak`, never `delay`. `NextHeld` is true when
  `HeldByInhibitors` is not empty: a user not in `wheel` learns that
  the entry is held, not who holds it.
- A console that becomes idle sends no signal (notes/logind-interface.md).
  So a console can stay in `HeldBySessions` after it is idle, up to the
  next signal or time of the service.
- One change of the state sends one `PropertiesChanged` per object, with
  every property of it that changed.
- A reader reads every property once (`GetAll`), then keeps the values
  that `PropertiesChanged` carries. It reads an object again only when a
  property is invalidated, or when the service comes on the bus again.
- When the service is not on the bus (a restart), a reader shows the
  state as not known. It follows the name by `NameOwnerChanged`, with
  a match of the name only (`arg0`).
- The name and the interface are `io.github.noktoborus.Drowse`. Two
  objects:
  - `/io/github/noktoborus/Drowse`: `State`, `ScheduledAction`,
    `NextHeld`, `PowerSource`, `Lid`.
    Anyone may read it: every user can learn of an action to come.
  - `/io/github/noktoborus/Drowse/HeldBy`: `HeldBySessions`,
    `HeldByInhibitors`. Only the group `wheel` may read it.
- The rights are set by the policy of D-Bus of the package, by the path
  of the object (`send_path`, `receive_path`), not by the service. The
  policy of D-Bus filters by the object, not by the name of a property.
- logind still shows its sessions and inhibitors to anyone
  (`ListInhibitors()`, `loginctl`). To close them is a setting of the
  system, not of drowse.

**No way round the system.** The service takes no commands of its own:
no method on the bus, no socket, no file to write to, no signal of its
own. Nothing can hold, hurry, cancel or change what it does except by the
mechanisms of the system, which check the rights themselves:

- to hold the action: an inhibitor of logind (`systemd-inhibit`,
  `drowse-inhibit`);
- to cancel an announce: any activity in a session;
- to change the settings: the settings file, then a restart of the
  unit (`systemctl`).

A reader learns of the announce by `State`: it becomes `announced`. It
does not work out the time of the announce. With `countdown: never`,
`State` becomes `held` when an entry waits for an inhibitor.

The service writes no `wall`: while an ssh session is open the system is
never idle, so no announce comes; a console sees it only when its text
screen is shown.

`ScheduleShutdown` of logind is not used: it has no sleep, it makes
`/run/nologin` and so stops a cancel by a new login, and it does not
check a `block` inhibitor at the time of the action
(notes/logind-schedule-shutdown.md).

## The journal

The service writes to the journal, short and clear, one line for one
change. A state that has not changed is not written again: a read by the
timer that finds the same is silent.

It writes:

- **A change of state**: idle or active, with the next section and its
  time; the power source or the lid, and whether the times moved or a
  section carried over; an entry held, with who holds it and why, and
  its release.
- **The announce and the action**: the action announced; the action
  done.
- **Why the action is cancelled**: a session in use (its id, user).
- **A source that is not there**: logind not on the bus; UPower not
  there, so the power is taken as AC; an action that logind refuses,
  with its answer.
- **A command of a section**: started; its `leave`; a stop that waits for
  a command that still runs; a command killed past its 15 s.

A source that comes back is written once, too.

Examples:

```
idle, on battery: poweroff in 17 min
announce: poweroff in 2 min
action: poweroff
cancel: session 4 (alice) in use
power: AC; the times moved
lid: closed; section main.battery.1 (power save) -> lid.battery.1 (power save): the same commands, none run; drowsed-stage@main.battery.1.service runs on
idle, on battery, the lid closed: power save in 0 s
idle, on AC: no action
upower: no answer; the power is taken as AC
section main.battery.1 (power save): started
held: main.battery.2 (poweroff) by drowse-inhibit (uid 1000): "held by hand"
released: main.battery.2 (poweroff) now
drowsed-stage@main.battery.1.service: the command still runs after 4 s; its leave waits
drowsed-stage@main.battery.1.service: the command ended; its leave runs late
suspend refused by logind: org.freedesktop.login1.SleepVerbNotSupported: Sleep verb 'suspend' is not configured or configuration is not supported by kernel
```

A time in the journal is the time left (`in 17 min`); the times on the
bus are of `CLOCK_MONOTONIC`.

The levels: a change of state is `info`; a cancel is `notice`; a source
not there, a refusal, a command that waits or is killed is `warning`.

## Settings

System settings only. A user has no settings of their own in the service.

The file is `/etc/drowse.yaml`. It has `sessions.classes`, an optional
`sections` library, and `main` and optional `lid` chains. Each chain has
`battery` and `ac` lists. A chain not set is empty. `lid` is used with
the lid closed (see "The lid"). With no file or an
empty file, all chains are empty and the service does nothing.

The keys of a section are:

| Key | Meaning | Default |
|---|---|---|
| `after` | idle time before announce; for the next section it starts at the entry before | `never` |
| `countdown` | time from the announce to entry; `never` means no announce | `never` for a command or empty section; `2m` for a logind action |
| `enter` | command to run on entry | none |
| `leave` | command to run when leaving a command section | none |
| `action` | action of logind; allowed only in the last section of a chain | none |
| `hint` | name shown on the bus and in the panel | `enter` or `action` as written |

`enter` and `leave` are commands. `enter: poweroff` runs the command
`poweroff`; use `action: poweroff` for the action of logind. An action
section cannot have `enter` or `leave`. A section with neither `enter`
nor `action` is entered with nothing done.

The file of the package is `etc/drowse.yaml`: on battery the power save
at 10 min of idle, then `poweroff` 38 min later with an announce of 2
min; on AC nothing; a library section `mute-powersave` and a `lid:` in a
comment, as examples.

`sections` is checked but not used by a chain. A chain uses an alias to
repeat a section, or `<<:` to change some of its keys. Times are seconds
or text such as `2m`, `1h 30min`, or `never`.

### The sessions that count

The mapping `sessions`, one key:

| Key | Meaning | Default |
|---|---|---|
| `classes` | the logind classes that count; a class not listed is left out; an empty list leaves all out | `[user, user-early, user-light, user-early-light, greeter]` |

- A name is compared with the class of a session as it is. Any name is
  taken, unchecked, so a class of a newer logind can be named.
- The default is the classes logind counts itself
  (`SESSION_CLASS_CAN_IDLE`, notes/logind-idle.md).
- A login screen (`greeter`) that sets no idle hint is never idle and
  keeps the system up while it is shown. `classes: [user, user-early,
  user-light, user-early-light]` leaves it out: with nobody logged in, the
  system is then idle.

### A change of the settings

- The service does not read its settings again while it runs; it is
  restarted. It keeps no state that matters: the idle state, the
  sessions, the inhibitors and the power source come again from logind
  and UPower at the start.
- At the start the count starts anew: the idle state starts no sooner
  than the start. An announce that ran before the restart is gone.
- The settings are in `/etc/drowse.yaml`. There are no drop-ins. With
  no file, the chains are empty.
- `drowsed-tuned enter <profile>...` (`/usr/libexec/drowse/`) keeps the
  profile and the mode of TuneD before and the profile set in the
  directory of the unit, then sets the profile. `drowsed-tuned leave`
  gives the profile before back (`tuned-adm auto_profile` for the mode
  `auto`, `tuned-adm off` for none), only when the profile set is still
  active in the mode `manual`. Else someone changed it: it stays, and
  the journal of the unit says so.
- The package gives the profiles of TuneD `cm5-powersave` and
  `cm5-powersave-no-radio` as examples (`examples/tuned/`, installed
  with the documentation); README.md says what they do.
- A unit `drowsed-config.path` watches `/etc/drowse.yaml`; on a change
  it runs `systemctl try-restart drowsed.service`.
- `drowsed.service`, `drowsed-config.path` and `drowsed-resume.service`
  are on by default: a preset file of the package enables them
  (`/usr/lib/systemd/system-preset/50-drowse.preset`). The files of the
  system are in `systemd/`, `preset/` and `dbus/`.

### Times

`after: 0`: entry is due as soon as the system is idle. If `countdown` is
set, the announce starts then. `after: never`: no entry.
`countdown: 0`: the announce is published and entry comes at once.
`countdown: never`: no announce; entry comes at `after`.

A time in the settings is one or more parts, each a number and a unit.
The parts are added. Spaces between the parts and between a number and
its unit may be there or not.

| Unit | Means |
|---|---|
| `s`, `sec` | seconds |
| `m`, `min` | minutes |
| `h`, `hr` | hours |

- A number with no unit is seconds.
- The form is that of a time span of systemd (`systemd.time(7)`), so a
  time reads the same as in a unit file.

Examples: `2h`; `1m 120s`, `1m120s` (both 3 minutes); `90` (90 seconds).

## Not the service

- What a logout leaves running. The service leaves `closing` sessions
  out; the documentation (README.md) still recommends
  `KillUserProcesses=yes`, with why.
- The screen and the lock: the idle program of the session, set by the
  user.
- Low and critical charge: UPower (`CriticalPowerAction=`).
- The interface: warnings, notifications, sound, the screen on at a
  warning, the state of the inhibitors, the lifetime of the inhibitors set
  by hand: `drowse-inhibit`, `drowse-status` and the module of waybar;
  a daemon of the interface is put off (the branch `plans`,
  `notify-daemon.md`).
