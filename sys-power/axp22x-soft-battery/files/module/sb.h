/* SPDX-License-Identifier: GPL-2.0 */
/*
 * axp22x_soft_battery: the battery of an AXP22x by a model of the device
 * tree, in place of axp20x_battery. The parts of the module:
 *
 *	main.c	the driver: probe, remove, suspend
 *	dt.c	the values of the device tree and their checks
 *	pmic.c	the registers of the PMIC
 *	level.c	the level of the charge by the voltage
 *	work.c	the samples, the silence of the AXP, the guard
 *	props.c	the power supply and its properties
 */

#ifndef SB_H
#define SB_H

#include <linux/atomic.h>
#include <linux/ktime.h>
#include <linux/mutex.h>
#include <linux/power_supply.h>
#include <linux/types.h>
#include <linux/workqueue.h>

#define SB_COMPATIBLE		"axp22x-soft-battery"
#define SB_PROP(name)		"axp22x-soft-battery," name

/* The means of the level are of the last three samples. */
#define SB_MEAN_SAMPLES		3

/* The curve of the charge has at most 16 pairs <uA percent>. */
#define SB_CV_MAX_PAIRS		16

/* R of the device tree stays within 10..500 mOhm. */
#define SB_R_MIN_UOHM		10000
#define SB_R_MAX_UOHM		500000

struct device;
struct device_node;
struct iio_channel;
struct regmap;

/* A point of the curve of the charge: the current, and the level at it. */
struct sb_cv_pair {
	u32 ua;
	u32 pct;
};

/* What get_property gives of the count: a copy of the last sample. */
struct sb_snapshot {
	bool valid;
	bool silent;
	int status;
	int capacity;
	int v_uv;
	int i_ua;
};

struct sb {
	struct device *dev;
	struct regmap *regmap;
	struct iio_channel *batt_v, *batt_chrg_i, *batt_dischrg_i;
	struct device_node *np;
	struct power_supply *psy;
	struct delayed_work work;

	/* The settings, read once at the probe. */
	u32 energy_full_uwh;
	u32 end_uv;
	u32 interval_ms;
	u32 fast_interval_ms;
	u32 fast_samples;
	u32 full_margin_uv;
	u32 full_current_ua;
	u32 silent_ms;
	u32 guard_ms;
	bool hw_protect;
	struct sb_cv_pair cv[SB_CV_MAX_PAIRS];
	int cv_len;
	u32 *reg_writes;
	int n_reg_writes;
	int full_uv;

	/* The state of the work, touched by the work only. */
	int v_uv[SB_MEAN_SAMPLES];
	int i_ua[SB_MEAN_SAMPLES];
	int n_mean;
	int pos_mean;
	unsigned int failed;
	ktime_t failed_since;
	bool silent;
	bool guarded;
	int prev_level;
	bool prev_supplied;
	int prev_status;
	bool at_zero;

	/* The samples left at a short period after the charger came or went. */
	atomic_t fast_left;

	/* The lock of snap. */
	struct mutex lock;
	struct sb_snapshot snap;
};

/* dt.c */
int sb_find_node(struct sb *sb);
int sb_read_dt(struct sb *sb);

/* pmic.c */
int sb_setup_pmic(struct sb *sb);
int sb_read_charging(struct sb *sb, bool *charging);

/* level.c */
int sb_level(struct sb *sb, int status, bool supplied, int v_uv, int i_ua);

/* work.c */
void sb_work(struct work_struct *work);
void sb_mean_reset(struct sb *sb);
void sb_sample_now(struct sb *sb);

/* props.c */
extern const struct power_supply_desc sb_desc;

#endif
