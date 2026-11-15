// SPDX-License-Identifier: GPL-2.0
/*
 * The power supply "axp22x-soft-battery" and its properties. The values
 * of design come from the device tree; the voltage, the current and the
 * power of now are read at the request (live.c); the others are a copy of
 * the last sample of the timer. No temp, no charge_*, no health: upower works out the
 * times and the health from the energy.
 */

#include <linux/device.h>
#include <linux/math64.h>
#include <linux/power_supply.h>

#include "sb.h"

/* The supply of the charger: the AC cell of the same PMIC. */
static char *sb_supplied_from[] = { "axp22x-ac" };

static const enum power_supply_property sb_props[] = {
	POWER_SUPPLY_PROP_PRESENT,
	POWER_SUPPLY_PROP_STATUS,
	POWER_SUPPLY_PROP_TECHNOLOGY,
	POWER_SUPPLY_PROP_CAPACITY,
	POWER_SUPPLY_PROP_VOLTAGE_NOW,
	POWER_SUPPLY_PROP_VOLTAGE_OCV,
	POWER_SUPPLY_PROP_CURRENT_NOW,
	POWER_SUPPLY_PROP_POWER_NOW,
	POWER_SUPPLY_PROP_ENERGY_NOW,
	POWER_SUPPLY_PROP_ENERGY_FULL,
	POWER_SUPPLY_PROP_ENERGY_FULL_DESIGN,
	POWER_SUPPLY_PROP_VOLTAGE_MIN_DESIGN,
	POWER_SUPPLY_PROP_VOLTAGE_MAX_DESIGN,
	POWER_SUPPLY_PROP_VOLTAGE_MIN,
	POWER_SUPPLY_PROP_VOLTAGE_MAX,
	POWER_SUPPLY_PROP_CONSTANT_CHARGE_CURRENT_MAX,
	POWER_SUPPLY_PROP_INTERNAL_RESISTANCE,
	POWER_SUPPLY_PROP_SCOPE,
};

/*
 * The values that do not change; -ENOENT for the others. voltage_min is
 * V_OFF and voltage_max the target of the charge, as written to the PMIC;
 * hwmon shows them as in0_min and in0_max.
 */
static int sb_get_fixed(struct sb *sb, struct power_supply_battery_info *info,
			enum power_supply_property psp,
			union power_supply_propval *val)
{
	switch (psp) {
	case POWER_SUPPLY_PROP_PRESENT:
		val->intval = 1;
		return 0;
	case POWER_SUPPLY_PROP_TECHNOLOGY:
		val->intval = info->technology;
		return 0;
	case POWER_SUPPLY_PROP_ENERGY_FULL:
		val->intval = sb->energy_full_uwh;
		return 0;
	case POWER_SUPPLY_PROP_ENERGY_FULL_DESIGN:
		val->intval = info->energy_full_design_uwh;
		return 0;
	case POWER_SUPPLY_PROP_VOLTAGE_MIN_DESIGN:
		val->intval = info->voltage_min_design_uv;
		return 0;
	case POWER_SUPPLY_PROP_VOLTAGE_MAX_DESIGN:
		val->intval = info->voltage_max_design_uv;
		return 0;
	case POWER_SUPPLY_PROP_VOLTAGE_MIN:
		val->intval = info->voltage_min_design_uv;
		return 0;
	case POWER_SUPPLY_PROP_VOLTAGE_MAX:
		val->intval = info->constant_charge_voltage_max_uv;
		return 0;
	case POWER_SUPPLY_PROP_CONSTANT_CHARGE_CURRENT_MAX:
		val->intval = info->constant_charge_current_max_ua;
		return 0;
	case POWER_SUPPLY_PROP_SCOPE:
		val->intval = POWER_SUPPLY_SCOPE_SYSTEM;
		return 0;
	default:
		return -ENOENT;
	}
}

/*
 * The values of the last sample. There are none before the first sample
 * and during the silence of the AXP: -ENODATA, and the uevent leaves
 * them out.
 */
static int sb_get_now(struct sb *sb, enum power_supply_property psp,
		      union power_supply_propval *val)
{
	guard(mutex)(&sb->lock);
	if (!sb->snap.valid || sb->snap.silent)
		return -ENODATA;

	switch (psp) {
	case POWER_SUPPLY_PROP_STATUS:
		val->intval = sb->snap.status;
		return 0;
	case POWER_SUPPLY_PROP_CAPACITY:
		val->intval = sb->snap.capacity;
		return 0;
	case POWER_SUPPLY_PROP_VOLTAGE_OCV:
		val->intval = sb->snap.ocv_uv;
		return 0;
	case POWER_SUPPLY_PROP_INTERNAL_RESISTANCE:
		val->intval = sb->snap.r_uohm;
		return 0;
	case POWER_SUPPLY_PROP_ENERGY_NOW:
		val->intval = div_u64((u64)sb->energy_full_uwh * sb->snap.capacity, 100);
		return 0;
	default:
		return -EINVAL;
	}
}

/* The values of now, read at the request (live.c). */
static bool sb_is_live(enum power_supply_property psp)
{
	return psp == POWER_SUPPLY_PROP_VOLTAGE_NOW ||
	       psp == POWER_SUPPLY_PROP_CURRENT_NOW ||
	       psp == POWER_SUPPLY_PROP_POWER_NOW;
}

static int sb_get_live(struct sb *sb, enum power_supply_property psp,
		       union power_supply_propval *val)
{
	int v_uv, i_ua, ret;

	ret = sb_live_get(sb, &v_uv, &i_ua);
	if (ret)
		return ret;

	if (psp == POWER_SUPPLY_PROP_VOLTAGE_NOW)
		val->intval = v_uv;
	else if (psp == POWER_SUPPLY_PROP_CURRENT_NOW)
		val->intval = i_ua;
	else
		val->intval = div_s64((s64)v_uv * i_ua, 1000000);
	return 0;
}

static int sb_get_property(struct power_supply *psy,
			   enum power_supply_property psp,
			   union power_supply_propval *val)
{
	struct sb *sb = power_supply_get_drvdata(psy);
	int ret = sb_get_fixed(sb, psy->battery_info, psp, val);

	if (ret != -ENOENT)
		return ret;
	return sb_is_live(psp) ? sb_get_live(sb, psp, val) : sb_get_now(sb, psp, val);
}

static void sb_external_power_changed(struct power_supply *psy)
{
	sb_sample_now(power_supply_get_drvdata(psy));
}

static void sb_tell_bound(struct sb *sb)
{
	struct power_supply_battery_info *info = sb->psy->battery_info;

	dev_info(sb->dev, "bound: charge current %d mA, V_OFF %d mV, full at %d mV; "
		 "R %d mOhm, energy_full %u mWh, curve of the charge %s, %d reg-writes\n",
		 info->constant_charge_current_max_ua / 1000,
		 info->voltage_min_design_uv / 1000,
		 info->constant_charge_voltage_max_uv / 1000,
		 info->factory_internal_resistance_uohm / 1000,
		 sb->energy_full_uwh / 1000,
		 sb->cv_len ? "from the device tree" : "none",
		 sb->n_reg_writes);
}

/*
 * The init of the power supply: the core has read monitored-battery and
 * not yet added the device. A bad value fails the registration, so no
 * battery is there with a bad model.
 */
static int sb_init(struct power_supply *psy)
{
	struct sb *sb = power_supply_get_drvdata(psy);
	int ret;

	sb->psy = psy;
	psy->supplied_from = sb_supplied_from;
	psy->num_supplies = ARRAY_SIZE(sb_supplied_from);

	ret = sb_read_dt(sb) ?: sb_setup_pmic(sb);
	if (ret)
		return ret;
	sb_r_init(sb);
	sb_tell_bound(sb);
	return 0;
}

const struct power_supply_desc sb_desc = {
	.name = "axp22x-soft-battery",
	.type = POWER_SUPPLY_TYPE_BATTERY,
	.properties = sb_props,
	.num_properties = ARRAY_SIZE(sb_props),
	.get_property = sb_get_property,
	.external_power_changed = sb_external_power_changed,
	.init = sb_init,
};
