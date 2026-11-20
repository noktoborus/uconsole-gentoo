# A swap in compressed memory of 1.5 GiB, zstd (zram-generator).

EAPI=8

inherit systemd

DESCRIPTION="A swap of 1.5 GiB in compressed memory"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	sys-apps/zram-generator
"

src_install() {
	insinto "$(systemd_get_utildir)"/zram-generator.conf.d
	newins "${FILESDIR}"/zram-generator.conf 50-uconsole.conf
}
