# The desktop of the uConsole on uconsole/system-base: sway and its pieces,
# the login by greetd and ReGreet, Plymouth, homes of homed, the keyring,
# PipeWire, drowse, the portals, Thunar, Bluetooth. Fonts, codecs and other
# programs are the choice of the user. The services are turned on by the
# user (repo-uconsole/README.md).

EAPI=8

DESCRIPTION="The desktop of the uConsole: sway, the login screen, the sound, the files"
HOMEPAGE="https://www.clockworkpi.com/uconsole"

LICENSE="metapackage"
SLOT="0"
KEYWORDS="arm64"

RDEPEND="
	uconsole/system-base
	gui-wm/sway
	uconsole/sway-settings
	uconsole/swayidle-sway
	uconsole/waybar-sway
	uconsole/rofi-sway
	uconsole/swaync-sway
	uconsole/grim-sway
	uconsole/wireplumber-sway
	uconsole/zyterm-sway
	uconsole/greeter-settings
	uconsole/plymouth-settings
	sys-apps/systemd[homed]
	sys-auth/pambase[homed,gnome-keyring]
	sys-fs/btrfs-progs
	uconsole/gnome-keyring-settings
	media-video/pipewire
	media-video/wireplumber
	media-video/raspberry-pi-wireplumber-hdmi
	media-sound/pwvucontrol
	sys-power/drowse
	gnome-extra/polkit-gnome
	sys-apps/xdg-desktop-portal
	sys-apps/xdg-desktop-portal-gtk
	gui-libs/xdg-desktop-portal-wlr
	x11-misc/xdg-user-dirs
	uconsole/thunar-settings
	gnome-base/gvfs
	sys-fs/udisks
	net-wireless/blueman
	gnome-extra/gnome-power-manager
"
