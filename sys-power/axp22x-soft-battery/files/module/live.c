// SPDX-License-Identifier: GPL-2.0
/*
 * The values of now at a request: voltage_now, current_now, power_now.
 * The count of the module runs on its timer (work.c); a reader of these
 * three gets the ADC as it is now, read at most once a second. The
 * samples of the timer count as reads too. These reads go into no mean
 * and no R.
 */

#include <linux/iio/consumer.h>
#include <linux/ktime.h>

#include "sb.h"

#define SB_LIVE_MS		1000

static bool sb_live_fresh(struct sb *sb)
{
	return sb->live_valid &&
	       ktime_ms_delta(ktime_get(), sb->live_time) < SB_LIVE_MS;
}

static void sb_live_put(struct sb *sb, int v_uv, int i_ua)
{
	sb->live_valid = true;
	sb->live_time = ktime_get();
	sb->live_v_uv = v_uv;
	sb->live_i_ua = i_ua;
}

/* A sample of the timer is the newest read too. */
void sb_live_store(struct sb *sb, int v_uv, int i_ua)
{
	guard(mutex)(&sb->live_lock);
	sb_live_put(sb, v_uv, i_ua);
}

static int sb_live_read_adc(struct sb *sb)
{
	int v_mv, chrg_ma, dischrg_ma, ret;

	ret = iio_read_channel_processed(sb->batt_v, &v_mv) ?:
	      iio_read_channel_processed(sb->batt_chrg_i, &chrg_ma) ?:
	      iio_read_channel_processed(sb->batt_dischrg_i, &dischrg_ma);
	if (ret)
		return ret;
	sb_live_put(sb, v_mv * 1000, (chrg_ma - dischrg_ma) * 1000);
	return 0;
}

/* No values before the first sample nor during the silence of the AXP. */
static bool sb_has_data(struct sb *sb)
{
	guard(mutex)(&sb->lock);
	return sb->snap.valid && !sb->snap.silent;
}

/*
 * The voltage and the current of now: the last read if it is younger than
 * a second, else a read of the ADC. With no data, or a failed read,
 * -ENODATA: the uevent leaves the value out and does not fail.
 */
int sb_live_get(struct sb *sb, int *v_uv, int *i_ua)
{
	if (!sb_has_data(sb))
		return -ENODATA;

	guard(mutex)(&sb->live_lock);
	if (!sb_live_fresh(sb) && sb_live_read_adc(sb))
		return -ENODATA;
	*v_uv = sb->live_v_uv;
	*i_ua = sb->live_i_ua;
	return 0;
}
