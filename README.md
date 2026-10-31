# The repository uconsole

The packages of the ClockworkPi uConsole that the gentoo repository
lacks. On the uConsole it is `/var/db/repos/uconsole`.

| package | what |
|---|---|
| `sys-kernel/uconsole-pi-kernel` | the kernels of the CM5 and the CM4 (USE `cm5`, `cm4`, `16k`) in `/boot`, their modules |
| `virtual/dist-kernel` | the packages of modules build again with each kernel |
| `sys-boot/uconsole-pi-firmware` | `/boot/config.txt`, `/boot/cmdline.txt`, the firmware of the CM4 |
| `gui-wm/uconsole-pi-desktop` | sway, greetd, the keyring and the agent of SSH, Flathub, DNS by resolved, mDNS by avahi, the presets of the services; its dependencies are the desktop |
| `dev-util/sccache-portage-sandbox` | the directories and the sandbox rule of sccache |

## Set up the uConsole from the base image

The base image has the kernel, the boot files, the firmware of the
Wi-Fi, and the sources of all of `@uconsole` in `/var/cache/distfiles`.
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

3. The set of the uConsole, binary packages from the binhost (an hour or
   more; a package the binhost lacks is built here, from the sources of
   the image):

   ```sh
   emerge -uDN @uconsole
   etc-update                 # the files of /etc and /boot the packages bring
   ```

4. The services of the desktop, as its presets say:

   ```sh
   systemctl preset NetworkManager.service systemd-resolved.service \
       avahi-daemon.service systemd-networkd.service \
       systemd-networkd-wait-online.service greetd.service \
       systemd-homed.service bluetooth.service
   systemctl --global preset pipewire.socket pipewire-pulse.socket wireplumber.service \
       gnome-keyring-daemon.socket gcr-ssh-agent.socket
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

7. `rm /etc/motd`, then `reboot`. gtkgreet asks for a user; its password
   unlocks the keyring (gnome-keyring, `/etc/pam.d/uconsole-greetd`), and
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
