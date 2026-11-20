# Read by Portage in each phase of each package. The patches of a program
# itself, kept beside this file in patches/<category>/<package>/; the head
# of each patch says why. They go on after src_prepare of the ebuild, as
# those of /etc/portage/patches would.

_uconsole_patches=${BASH_SOURCE[0]%/*}/patches

post_src_prepare() {
	local d=${_uconsole_patches}/${CATEGORY}/${PN}
	[[ -d ${d} ]] || return 0
	eapply "${d}"
}
