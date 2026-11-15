// SPDX-License-Identifier: GPL-2.0
/*
 * R of the cells, learned while the module runs, as adc-battery-helper.c
 * of the kernel does. On a step of the current between two samples, all
 * the change of the voltage is put on R. R starts at
 * factory-internal-resistance-micro-ohms at each probe; the level takes
 * the mean of the last steps taken.
 */

#include <linux/minmax.h>
#include <linux/power_supply.h>

#include "sb.h"

/* A step of at least 65 mA, over 3.7 V, as the kernel takes it. */
#define SB_R_STEP_UA		65000
#define SB_R_MIN_V_UV		3700000

/* The R of the device tree, and the mean of the window with it. */
void sb_r_init(struct sb *sb)
{
	sb->r_uohm = sb->psy->battery_info->factory_internal_resistance_uohm;
	sb->r_steps[0] = sb->r_uohm;
	sb->r_n = 1;
	sb->r_pos = 1 % SB_R_WINDOW;
	sb->r_prev_valid = false;
}

/* The next step starts from the next sample: the one before is too old. */
void sb_r_forget_step(struct sb *sb)
{
	sb->r_prev_valid = false;
}

/*
 * A step counts when the charger is the same at both samples, the
 * voltage is not low, and the current changed enough.
 */
static bool sb_r_is_step(struct sb *sb, bool supplied, int v_uv, int i_ua)
{
	return sb->r_prev_valid && supplied == sb->r_prev_supplied &&
	       v_uv >= SB_R_MIN_V_UV &&
	       abs(i_ua - sb->r_prev_i_ua) >= SB_R_STEP_UA;
}

/* A value out of 2/3..4/3 of the mean is a step of the cells, not of R. */
static bool sb_r_near_mean(struct sb *sb, s64 r)
{
	return r * 3 >= (s64)sb->r_uohm * 2 && r * 3 <= (s64)sb->r_uohm * 4;
}

static void sb_r_take(struct sb *sb, int r)
{
	s64 sum = 0;
	int i;

	sb->r_steps[sb->r_pos] = r;
	sb->r_pos = (sb->r_pos + 1) % SB_R_WINDOW;
	sb->r_n = min(sb->r_n + 1, SB_R_WINDOW);
	for (i = 0; i < sb->r_n; i++)
		sum += sb->r_steps[i];
	sb->r_uohm = clamp_t(int, div_s64(sum, sb->r_n), SB_R_MIN_UOHM, SB_R_MAX_UOHM);
}

/* A sample: its step from the one before, if it is one. */
void sb_r_add(struct sb *sb, bool supplied, int v_uv, int i_ua)
{
	s64 r;

	if (sb_r_is_step(sb, supplied, v_uv, i_ua)) {
		r = div_s64((s64)abs(v_uv - sb->r_prev_v_uv) * 1000000,
			    abs(i_ua - sb->r_prev_i_ua));
		if (sb_r_near_mean(sb, r))
			sb_r_take(sb, r);
	}
	sb->r_prev_valid = true;
	sb->r_prev_supplied = supplied;
	sb->r_prev_v_uv = v_uv;
	sb->r_prev_i_ua = i_ua;
}
