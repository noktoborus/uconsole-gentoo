// SPDX-License-Identifier: GPL-2.0
/*
 * The values of the device tree. The module has no default: each value is
 * in the node "axp22x-soft-battery" or in the simple-battery of its
 * monitored-battery, and a value missing or bad fails the probe.
 */

#include <linux/device.h>
#include <linux/array_size.h>
#include <linux/limits.h>
#include <linux/math.h>
#include <linux/of.h>
#include <linux/power_supply.h>
#include <linux/slab.h>
#include <linux/stddef.h>

#include "sb.h"

/* Bounds of the checks, not settings. */
#define SB_END_MAX_UV		3800000

static int sb_refuse(struct sb *sb, const char *name, long long val,
		     const char *why)
{
	dev_err(sb->dev, "%s refused: %lld, %s\n", name, val, why);
	return -EINVAL;
}

static int sb_missing(struct sb *sb, const char *name)
{
	dev_err(sb->dev, "%s missing\n", name);
	return -EINVAL;
}

static int sb_read_u32(struct sb *sb, const char *name, u32 *val)
{
	return of_property_read_u32(sb->np, name, val) ? sb_missing(sb, name) : 0;
}

static int sb_check_range(struct sb *sb, const char *name, u32 val,
			  u32 min, u32 max)
{
	if (val >= min && val <= max)
		return 0;
	dev_err(sb->dev, "%s refused: %u, not in %u..%u\n", name, val, min, max);
	return -EINVAL;
}

/*
 * The node of the values, found by its compatible, not by a path: the
 * same overlay serves each device. It must be the one node so.
 */
int sb_find_node(struct sb *sb)
{
	struct device_node *other;

	sb->np = of_find_compatible_node(NULL, NULL, SB_COMPATIBLE);
	if (!sb->np)
		return dev_err_probe(sb->dev, -EINVAL,
				     "no node \"" SB_COMPATIBLE "\"\n");

	other = of_find_compatible_node(of_node_get(sb->np), NULL, SB_COMPATIBLE);
	if (!other)
		return 0;
	of_node_put(other);
	return dev_err_probe(sb->dev, -EINVAL,
			     "more than one node \"" SB_COMPATIBLE "\"\n");
}

/* A value between two steps of a register takes a step, with a line. */
static void sb_tell_rounded(struct sb *sb, const char *name, int *val, int to)
{
	if (*val == to)
		return;
	dev_warn(sb->dev, "%s rounded: %d to %d\n", name, *val, to);
	*val = to;
}

/*
 * V_OFF of the AXP22x: 2.6-3.3 V by 0.1 V. Between two steps it goes up:
 * the cut comes sooner, never later.
 */
static int sb_round_voltage_min(struct sb *sb, int *uv)
{
	const char *name = "voltage-min-design-microvolt";

	if (*uv < 2600000 || *uv > 3300000)
		return sb_refuse(sb, name, *uv, "not in 2600000..3300000");
	sb_tell_rounded(sb, name, uv, 2600000 + roundup(*uv - 2600000, 100000));
	return 0;
}

/*
 * The target of the charge: the four codes of the AXP22x. Between two
 * codes it goes down: the cells are never charged over the value.
 */
static int sb_round_charge_voltage(struct sb *sb, int *uv)
{
	static const int codes[] = { 4240000, 4220000, 4200000, 4100000 };
	const char *name = "constant-charge-voltage-max-microvolt";
	int i;

	if (*uv > codes[0])
		return sb_refuse(sb, name, *uv, "over 4240000");
	for (i = 0; i < ARRAY_SIZE(codes); i++) {
		if (*uv >= codes[i]) {
			sb_tell_rounded(sb, name, uv, codes[i]);
			return 0;
		}
	}
	return sb_refuse(sb, name, *uv, "under 4100000");
}

/*
 * The current of the charge of the AXP22x: 300-2550 mA by 150 mA. Between
 * two steps it goes down, as axp20x_battery.c does.
 */
static int sb_round_charge_current(struct sb *sb, int *ua)
{
	const char *name = "constant-charge-current-max-microamp";

	if (*ua < 300000 || *ua > 2550000)
		return sb_refuse(sb, name, *ua, "not in 300000..2550000");
	sb_tell_rounded(sb, name, ua, 300000 + rounddown(*ua - 300000, 150000));
	return 0;
}

/*
 * The curve of the discharge. The core reads the table only when
 * ocv-capacity-celsius is there; its voltage must fall point by point.
 */
static int sb_check_ocv_table(struct sb *sb, struct power_supply_battery_info *info)
{
	int i, prev = INT_MAX;

	if (!info->ocv_table[0] || info->ocv_table_size[0] < 2)
		return sb_missing(sb, "ocv-capacity-table-0 or ocv-capacity-celsius");

	for (i = 0; i < info->ocv_table_size[0]; i++) {
		if (info->ocv_table[0][i].ocv >= prev)
			return sb_refuse(sb, "ocv-capacity-table-0",
					 info->ocv_table[0][i].ocv,
					 "the voltage does not fall");
		prev = info->ocv_table[0][i].ocv;
	}
	return 0;
}

/*
 * The simple-battery of monitored-battery, as the core read it. The values
 * of the registers are rounded in place, so the properties show what the
 * PMIC was given.
 */
static int sb_check_battery_info(struct sb *sb)
{
	struct power_supply_battery_info *info = sb->psy->battery_info;
	int ret;

	if (!info)
		return sb_missing(sb, "monitored-battery");
	if (info->technology == POWER_SUPPLY_TECHNOLOGY_UNKNOWN)
		return sb_missing(sb, "device-chemistry");
	if (info->energy_full_design_uwh <= 0)
		return sb_missing(sb, "energy-full-design-microwatt-hours");
	if (info->voltage_max_design_uv <= 0)
		return sb_missing(sb, "voltage-max-design-microvolt");

	ret = sb_round_voltage_min(sb, &info->voltage_min_design_uv) ?:
	      sb_round_charge_voltage(sb, &info->constant_charge_voltage_max_uv) ?:
	      sb_round_charge_current(sb, &info->constant_charge_current_max_ua);
	if (ret)
		return ret;

	if (info->factory_internal_resistance_uohm < SB_R_MIN_UOHM ||
	    info->factory_internal_resistance_uohm > SB_R_MAX_UOHM)
		return sb_refuse(sb, "factory-internal-resistance-micro-ohms",
				 info->factory_internal_resistance_uohm,
				 "missing or not 10000..500000");

	return sb_check_ocv_table(sb, info);
}

/* A pair of the curve of the charge: the current falls, the level rises. */
static int sb_check_cv_pair(struct sb *sb, int i)
{
	const char *name = SB_PROP("charge-cv-table");

	if (sb->cv[i].pct > 100)
		return sb_refuse(sb, name, sb->cv[i].pct, "a percent over 100");
	if (i && (sb->cv[i].ua >= sb->cv[i - 1].ua ||
		  sb->cv[i].pct <= sb->cv[i - 1].pct))
		return sb_refuse(sb, name, i,
				 "the current must fall and the percent rise");
	return 0;
}

/* The curve of the charge is the one value that may be missing: no curve. */
static int sb_read_cv_table(struct sb *sb)
{
	const char *name = SB_PROP("charge-cv-table");
	u32 raw[SB_CV_MAX_PAIRS * 2];
	int n, i, ret;

	n = of_property_count_u32_elems(sb->np, name);
	if (n == -EINVAL)
		return 0;
	if (n < 2 || n > SB_CV_MAX_PAIRS * 2 || n % 2)
		return sb_refuse(sb, name, n, "not 1..16 pairs");

	ret = of_property_read_u32_array(sb->np, name, raw, n);
	if (ret)
		return sb_refuse(sb, name, ret, "not read");

	sb->cv_len = n / 2;
	for (i = 0; i < sb->cv_len; i++) {
		sb->cv[i].ua = raw[2 * i];
		sb->cv[i].pct = raw[2 * i + 1];
		ret = sb_check_cv_pair(sb, i);
		if (ret)
			return ret;
	}
	return 0;
}

/* A write <reg mask value>: each a byte, the value in the mask. */
static int sb_check_reg_write(struct sb *sb, const u32 *w, int i)
{
	if (w[0] <= 0xff && w[1] <= 0xff && w[2] <= 0xff && !(w[2] & ~w[1]))
		return 0;
	return sb_refuse(sb, SB_PROP("reg-writes"), i,
			 "a value over 0xff, or a bit out of the mask");
}

/* The writes of the registers, done in order at each probe. */
static int sb_read_reg_writes(struct sb *sb)
{
	const char *name = SB_PROP("reg-writes");
	int n, i, ret;

	n = of_property_count_u32_elems(sb->np, name);
	if (n == -EINVAL)
		return sb_missing(sb, name);
	if (n < 3 || n % 3)
		return sb_refuse(sb, name, n, "not triples <reg mask value>");

	sb->reg_writes = devm_kmalloc_array(sb->dev, n, sizeof(u32), GFP_KERNEL);
	if (!sb->reg_writes)
		return -ENOMEM;
	ret = of_property_read_u32_array(sb->np, name, sb->reg_writes, n);
	if (ret)
		return sb_refuse(sb, name, ret, "not read");

	sb->n_reg_writes = n / 3;
	for (i = 0; i < sb->n_reg_writes; i++) {
		ret = sb_check_reg_write(sb, &sb->reg_writes[3 * i], i);
		if (ret)
			return ret;
	}
	return 0;
}

/* A setting of the node: its field, and the values it may take. */
struct sb_setting {
	const char *name;
	size_t offset;
	u32 min;
	u32 max;
};

#define SB_SETTING(prop, field, lo, hi) \
	{ SB_PROP(prop), offsetof(struct sb, field), lo, hi }

static const struct sb_setting sb_settings[] = {
	SB_SETTING("energy-full-microwatt-hours", energy_full_uwh, 1, INT_MAX),
	SB_SETTING("end-microvolt", end_uv, 0, SB_END_MAX_UV),
	SB_SETTING("sample-interval-ms", interval_ms, 1000, 600000),
	SB_SETTING("full-margin-microvolt", full_margin_uv, 1, 500000),
	SB_SETTING("full-current-microamp", full_current_ua, 1, 1000000),
	SB_SETTING("silent-samples", silent_samples, 1, 1000),
	SB_SETTING("guard-samples", guard_samples, 2, 1000),
};

static int sb_read_settings(struct sb *sb)
{
	const struct sb_setting *s;
	u32 *val;
	int ret;

	for (s = sb_settings; s < sb_settings + ARRAY_SIZE(sb_settings); s++) {
		val = (u32 *)((char *)sb + s->offset);
		ret = sb_read_u32(sb, s->name, val) ?:
		      sb_check_range(sb, s->name, *val, s->min, s->max);
		if (ret)
			return ret;
	}
	return 0;
}

/* The end may not be under V_OFF; the guard comes after the silence. */
static int sb_check_relations(struct sb *sb)
{
	return sb_check_range(sb, SB_PROP("end-microvolt"), sb->end_uv,
			      sb->psy->battery_info->voltage_min_design_uv,
			      SB_END_MAX_UV) ?:
	       sb_check_range(sb, SB_PROP("guard-samples"), sb->guard_samples,
			      sb->silent_samples + 1, 1000);
}

/*
 * All the values, each checked. It runs when the core of the power
 * supply has read monitored-battery into psy->battery_info.
 */
int sb_read_dt(struct sb *sb)
{
	int ret;

	ret = sb_check_battery_info(sb) ?: sb_read_settings(sb) ?:
	      sb_check_relations(sb) ?: sb_read_cv_table(sb) ?:
	      sb_read_reg_writes(sb);
	if (ret)
		return ret;

	sb->full_uv = sb->psy->battery_info->constant_charge_voltage_max_uv -
		      sb->full_margin_uv;
	return 0;
}
