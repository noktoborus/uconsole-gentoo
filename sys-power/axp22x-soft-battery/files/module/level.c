// SPDX-License-Identifier: GPL-2.0
/*
 * The level of the charge, always by the voltage, on the curves of the
 * device tree. The module counts no charge.
 */

#include <linux/math64.h>
#include <linux/minmax.h>
#include <linux/power_supply.h>

#include "sb.h"

/*
 * The level on the curve of the charge: a line between two pairs; under
 * the last pair, its level.
 */
static int sb_level_cv(struct sb *sb, int ua)
{
	const struct sb_cv_pair *a, *b;
	int i;

	for (i = 0; i + 1 < sb->cv_len; i++) {
		a = &sb->cv[i];
		b = &sb->cv[i + 1];
		if (ua > b->ua)
			return a->pct + (int)div_s64((s64)(a->ua - ua) * (b->pct - a->pct),
						     a->ua - b->ua);
	}
	return sb->cv[sb->cv_len - 1].pct;
}

/*
 * The phase of constant voltage: no discharge, the voltage near the
 * target, a current into the cells, and a curve of the charge for this
 * current. Over its first pair the curve of the discharge is used.
 */
static bool sb_on_cv_curve(struct sb *sb, int status, int v_uv, int i_ua)
{
	return status != POWER_SUPPLY_STATUS_DISCHARGING &&
	       v_uv >= sb->full_uv && sb->cv_len &&
	       i_ua > 0 && i_ua <= (int)sb->cv[0].ua;
}

/* OCV = V - I * R, with I positive on a charge, R of the device tree. */
static int sb_ocv(struct sb *sb, int v_uv, int i_ua)
{
	int r_uohm = sb->psy->battery_info->factory_internal_resistance_uohm;

	return v_uv - (int)div_s64((s64)i_ua * r_uohm, 1000000);
}

/* The level on the curve of the discharge; 0 % at the end voltage. */
static int sb_level_ocv(struct sb *sb, int status, int v_uv, int i_ua)
{
	struct power_supply_battery_info *info = sb->psy->battery_info;
	int level;

	if (status == POWER_SUPPLY_STATUS_DISCHARGING && v_uv <= (int)sb->end_uv)
		return 0;

	level = power_supply_batinfo_ocv2cap(info, sb_ocv(sb, v_uv, i_ua),
					     info->ocv_temp[0]);
	return clamp(level, 0, 100);
}

/* The level of a sample, by the means of the voltage and the current. */
int sb_level(struct sb *sb, int status, int v_uv, int i_ua)
{
	if (status == POWER_SUPPLY_STATUS_FULL)
		return 100;
	if (sb_on_cv_curve(sb, status, v_uv, i_ua))
		return sb_level_cv(sb, i_ua);
	return sb_level_ocv(sb, status, v_uv, i_ua);
}
