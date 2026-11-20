# The repository uconsole

The packages of the ClockworkPi uConsole that the gentoo repository
lacks. On the uConsole it is `/var/db/repos/uconsole`.

| package | what |
|---|---|
| `sys-kernel/uconsole-pi-kernel` | the kernels of the CM5 and the CM4 (USE `cm5`, `cm4`, `cm5-16k`) in `/boot`, their modules |
| `virtual/dist-kernel` | the packages of modules build again with each kernel |
| `virtual/linux-sources` | nothing: it points to `sys-kernel/uconsole-pi-kernel`, so a package of a module does not pull `gentoo-sources` |
| `sys-boot/uconsole-pi-firmware` | `/boot/config.txt`, `/boot/cmdline.txt`, the firmware of the CM4 |
| `app-portage/portage-ccache` | emerge builds C and C++ under ccache: `/etc/portage/env/portage-ccache.conf` (`FEATURES=ccache`, `CCACHE_DIR`) for `*/*` of `package.env`, `/var/cache/ccache` |
| `app-portage/portage-sccache` | emerge builds Rust under sccache: `/etc/portage/env/portage-sccache.conf` for `*/*` of `package.env`, none for `dev-lang/rust` and `dev-util/sccache`, the sandbox rule, `/var/cache/sccache`, `/var/tmp/portage/sccache` by tmpfiles |
| `uconsole/system-base` | the system with no desktop: the kernel, the firmware, the battery, NetworkManager, DNS, logind, zram, the UCM, the caches of emerge |
| `uconsole/sway-desktop` | the desktop on `system-base`: sway and its pieces, greetd and ReGreet, Plymouth, homed, the keyring, PipeWire, drowse, the portals, Thunar, blueman; fonts, codecs and other programs are the choice of the user |
| `uconsole/sway-settings` | `/etc/sway/config.d/40-uconsole-output.conf` (the panel), `50-uconsole.conf` (the session of systemd), `50-uconsole-backlight.conf`, `50-uconsole-bindings.conf` (the lid), `sway-session.target`, `/usr/bin/backlight` |
| `uconsole/*-sway` | a file of `/etc/sway/config.d/60-<program>.conf` each: swayidle (lock, the panel off), waybar, rofi (Super+d), swaync (Super+n), grim (Print), wireplumber (the keys of the volume), zyterm (the terminal, Super+Enter) |
| `uconsole/greeter-settings` | `/etc/greetd/uconsole.toml`, `/etc/greetd/sway-config` (ReGreet in sway; power off after 10 min idle), `greetd.service.d` |
| `uconsole/logind-settings` | `logind.conf.d/50-uconsole.conf`: the power key powers off; logind ignores the lid, the other keys and idle |
| `uconsole/*-settings` | plymouth (`--retain-splash`), thunar (thunar-volman, autostart), networkmanager (`.local`), resolved (mDNS resolve, `/etc/resolv.conf`), gnome-keyring (the agent of SSH), flatpak (Flathub), zram-generator (1.5 GiB), google-chrome (the hook of Portage: ECH off, an entry with WebGPU; `PDEPEND` on Chrome) |
| `media-libs/uconsole-pi-ucm` | the UCM of the sound card uConsole |
| `x11-terms/zyterm` | `zyterm` and `zyt-script`: a GUI terminal for serial consoles and the local shell (egui, wgpu); the terminal of the desktop |
| `gui-libs/greetd`, `gui-apps/waybar`, `dev-libs/date`, `gui-libs/xdg-desktop-portal-wlr`, `sys-apps/zram-generator`, `dev-libs/granite`, `media-sound/pwvucontrol` | copies of the gentoo ebuilds, stable on amd64 and arm64: gentoo has no stable arm64 for them |
| `gui-apps/ReGreet` | `/usr/bin/regreet`, the greeter of greetd (GTK4); with USE `systemd` its directories `/var/lib/regreet`, `/var/log/regreet` by tmpfiles; a copy of the GURU ebuild, stable on amd64 and arm64; not yet tried on the device |
| `sys-power/axp22x-soft-battery` | the module `axp22x_soft_battery` (the battery by a model of the device tree), `/etc/modprobe.d/axp22x-soft-battery.conf` (blacklist of `axp20x_battery`), `/boot/overlays/axp22x-soft-battery.dtbo`; not yet tried on the device; its source is axp22x-soft-battery |
| `sys-power/axp22x-soft-battery-daemon` | `/usr/bin/axp22x-soft-battery` (the learning of the cells, `gen-dts`, `import`), its service and preset; not yet tried on the device |
| `sys-power/drowse` | `drowsed` (while every session of logind is idle, the actions of its stages: a command and its undo, an action of power; its state on the system bus), `drowse-inhibit`, `drowse-status` (also for waybar), the units, the preset, the policy of D-Bus; not yet tried on the device; its source is drowse |
| `sys-apps/systemctl-tui` | `/usr/bin/systemctl-tui`: a TUI of the services of systemd and of their logs; built from crates.io; built here on amd64, not yet tried on the device |
| `media-video/raspberry-pi-wireplumber-hdmi` | the rule of udev `90-raspberry-pi-wireplumber-hdmi.rules`: on a hotplug of DRM, `/usr/libexec/raspberry-pi-wireplumber-hdmi-udev` starts the oneshot user unit `raspberry-pi-wireplumber-hdmi.service` in the manager of each user; it asks WirePlumber (`ReserveDevice1`) to make again each `vc4hdmi` card whose profiles do not fit the display in (no HDMI profile, or another ELD), so the HDMI output comes (`notes/hdmi-audio-hotplug.md`); tried on the CM5 |

## Set up the uConsole from the base image

The base image has the kernel, the boot files, the firmware of the
Wi-Fi, and the sources of all of `@world` of the full system in `/var/cache/distfiles`.
Log in as root (password `root`).

1. NetworkManager, built here from those sources (no network needed),
   then Wi-Fi:

   ```sh
   FEATURES=-getbinpkg emerge -1 net-misc/networkmanager   # no binhost yet
   systemctl enable --now NetworkManager.service
   nmtui                      # or: nmcli device wifi connect <ssid> --ask
   ```

2. The binhost: `sync-uri` in `/etc/portage/binrepos.conf/uconsole.conf`
   is the address of the machine with the containers. Check it:

   ```sh
   curl -sI http://<host>:8080/cm5/Packages     # 200 OK
   ```

3. The desktop of the uConsole, binary packages from the binhost (an hour or
   more; a package the binhost lacks is built here, from the sources of
   the image):

   ```sh
   emerge -uDN uconsole/sway-desktop @world
   etc-update                 # the files of /etc and /boot the packages bring
   ```

4. The services of the desktop; no package turns them on:

   ```sh
   systemctl enable NetworkManager.service systemd-resolved.service \
       avahi-daemon.service greetd.service systemd-homed.service \
       bluetooth.service fstrim.timer
   systemctl disable systemd-networkd.service systemd-networkd-wait-online.service
   systemctl --global enable pipewire.socket pipewire-pulse.socket wireplumber.service \
       swaync.service gnome-keyring-daemon.socket gcr-ssh-agent.socket
   systemctl set-default graphical.target
   ```

5. DNS by systemd-resolved (NetworkManager wrote its own
   `/etc/resolv.conf` before):

   ```sh
   systemctl start systemd-resolved.service
   ln -snf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf
   systemctl restart NetworkManager.service
   ```

6. A user, with its home in LUKS (homed); the password is the one of
   the login:

   ```sh
   homectl create user --storage=luks --member-of=wheel,video,audio,input
   ```

7. `rm /etc/motd`, then `reboot`. ReGreet asks for a user; its password
   unlocks the keyring (gnome-keyring, `pam_gnome_keyring` of pambase), and
   `SSH_AUTH_SOCK` is the agent of gcr (`gcr-ssh-agent.socket`).

More, if wanted:

```sh
passwd                                            # the password of root
flatpak install flathub io.github.kolunmi.Bazaar  # a store of Flathub
```

## Update

```sh
emerge --sync && emerge -uDN @world
```

The binhost must have the repository of the same date: sync the builder
first.
