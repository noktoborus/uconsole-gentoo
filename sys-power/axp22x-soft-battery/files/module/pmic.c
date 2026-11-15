// SPDX-License-Identifier: GPL-2.0
/*
 * The registers of the AXP22x (the datasheet of the AXP228, REG 01H, 31H,
 * 33H). The AXP forgets them at a cut of the power, so the module sets
 * them at each probe, from the device tree only.
 */

#include <linux/bits.h>
#include <linux/device.h>
#include <linux/mfd/axp20x.h>
#include <linux/power_supply.h>
#include <linux/regmap.h>

#include "sb.h"

#define SB_OP_CHARGING		BIT(6)
#define SB_V_OFF_MASK		GENMASK(2, 0)
#define SB_CHRG_VOLT_MASK	GENMASK(6, 5)
#define SB_CHRG_VOLT_SHIFT	5
#define SB_CHRG_CURR_MASK	GENMASK(3, 0)

/*
 * V_OFF in 0x31 [2:0]: 000 is 2.6 V, each step 0.1 V, 111 is 3.3 V (the
 * datasheet of the AXP228 V1.1, REG 31H; axp20x_battery.c reads it so).
 */
static unsigned int sb_v_off_code(int uv)
{
	return (uv - 2600000) / 100000;
}

/* The target of the charge in 0x33 [6:5], as axp20x_battery.c has them. */
static unsigned int sb_charge_voltage_code(int uv)
{
	switch (uv) {
	case 4100000:
		return 0;
	case 4220000:
		return 1;
	case 4200000:
		return 2;
	default:
		return 3;
	}
}

/* The current of the charge in 0x33 [3:0]: 300 mA + n * 150 mA. */
static unsigned int sb_charge_current_code(int ua)
{
	return (ua - 300000) / 150000;
}

static int sb_write_battery(struct sb *sb)
{
	struct power_supply_battery_info *info = sb->psy->battery_info;
	unsigned int ctrl1;

	ctrl1 = sb_charge_voltage_code(info->constant_charge_voltage_max_uv) <<
		SB_CHRG_VOLT_SHIFT |
		sb_charge_current_code(info->constant_charge_current_max_ua);

	return regmap_update_bits(sb->regmap, AXP20X_V_OFF, SB_V_OFF_MASK,
				  sb_v_off_code(info->voltage_min_design_uv)) ?:
	       regmap_update_bits(sb->regmap, AXP20X_CHRG_CTRL1,
				  SB_CHRG_VOLT_MASK | SB_CHRG_CURR_MASK, ctrl1);
}

static int sb_write_reg_writes(struct sb *sb)
{
	const u32 *w;
	int i, ret;

	for (i = 0; i < sb->n_reg_writes; i++) {
		w = &sb->reg_writes[3 * i];
		ret = regmap_update_bits(sb->regmap, w[0], w[1], w[2]);
		if (ret)
			return ret;
	}
	return 0;
}

/* V_OFF and the charge from the battery, then reg-writes in order. */
int sb_setup_pmic(struct sb *sb)
{
	int ret = sb_write_battery(sb) ?: sb_write_reg_writes(sb);

	if (ret)
		dev_err(sb->dev, "set up of the PMIC failed: %d\n", ret);
	return ret;
}

/* Bit 6 of 0x01: the charger charges the cells now. */
int sb_read_charging(struct sb *sb, bool *charging)
{
	unsigned int op;
	int ret = regmap_read(sb->regmap, AXP20X_PWR_OP_MODE, &op);

	if (ret)
		return ret;
	*charging = op & SB_OP_CHARGING;
	return 0;
}
