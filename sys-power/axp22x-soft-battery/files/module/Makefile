# SPDX-License-Identifier: GPL-2.0
# The module out of the tree of the kernel: make KDIR=<the build tree>.
obj-m := axp22x_soft_battery.o
axp22x_soft_battery-y := main.o dt.o pmic.o level.o work.o props.o

KDIR ?= /lib/modules/$(shell uname -r)/build

modules:
	$(MAKE) -C $(KDIR) M=$(CURDIR) modules

clean:
	$(MAKE) -C $(KDIR) M=$(CURDIR) clean

.PHONY: modules clean
