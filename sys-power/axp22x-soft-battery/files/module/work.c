// SPDX-License-Identifier: GPL-2.0
/*
 * The samples: one run of the work each sample-interval-ms. It reads the
 * ADC without the lock, works out the status and the level, then puts
 * them into the snapshot under the lock.
 *
 * When all reads find no AXP on the bus, the sample fails. When the
 * samples fail for silent-ms, the values give -ENODATA; for guard-ms, the
 * module powers the system off, the one power off it does, unless
 * no-hardware-protect is set.
 */

#include <linux/device.h>
#include <linux/iio/consumer.h>
#include <linux/math64.h>
#include <linux/minmax.h>
#include <linux/power_supply.h>
#include <linux/reboot.h>
#include <linux/timer.h>

#include "sb.h"

#define SB_GUARD_MS		20000

/* One sample of the ADC, in mV and mA. */
struct sb_sample {
	int v_mv;
	int chrg_ma;
	int dischrg_ma;
};

struct sb_means {
	int v_uv;
	int i_ua;
};

/*
 * The next run. After the charger comes or goes, the current and bit 6 of
 * 0x01 take a moment to settle; at the start the means are empty: so
 * fast-samples runs at fast-interval-ms follow, then the period.
 */
static void sb_schedule(struct sb *sb)
{
	unsigned long delay;

	if (atomic_dec_if_positive(&sb->fast_left) >= 0)
		delay = msecs_to_jiffies(sb->fast_interval_ms);
	else
		delay = round_jiffies_relative(msecs_to_jiffies(sb->interval_ms));
	queue_delayed_work(system_power_efficient_wq, &sb->work, delay);
}

/*
 * A run now, not at the end of the period, and a few more soon after it:
 * the charger came or went, or the work starts at the probe or a resume.
 */
void sb_sample_now(struct sb *sb)
{
	atomic_set(&sb->fast_left, sb->fast_samples);
	mod_delayed_work(system_power_efficient_wq, &sb->work, 0);
}

/* The means and the level start again from the voltage. */
void sb_mean_reset(struct sb *sb)
{
	sb->n_mean = 0;
	sb->pos_mean = 0;
	sb->prev_level = -1;
}

/* The three reads of the ADC; their errors, each. */
static void sb_read_adc(struct sb *sb, struct sb_sample *s, int err[3])
{
	err[0] = iio_read_channel_processed(sb->batt_v, &s->v_mv);
	err[1] = iio_read_channel_processed(sb->batt_chrg_i, &s->chrg_ma);
	err[2] = iio_read_channel_processed(sb->batt_dischrg_i, &s->dischrg_ma);
}

/* No AXP answers on the bus: all three reads end in -ENXIO. */
static bool sb_axp_silent(const int err[3])
{
	return err[0] == -ENXIO && err[1] == -ENXIO && err[2] == -ENXIO;
}

/* The time since the first failed sample of the row, in ms. */
static s64 sb_failed_ms(struct sb *sb)
{
	return ktime_ms_delta(ktime_get(), sb->failed_since);
}

static void sb_enter_silence(struct sb *sb, int err)
{
	sb->silent = true;
	scoped_guard(mutex, &sb->lock)
		sb->snap.silent = true;
	sb_mean_reset(sb);
	dev_warn(sb->dev, "AXP silent: %u samples failed in %lld ms, last %d\n",
		 sb->failed, sb_failed_ms(sb), err);
	power_supply_changed(sb->psy);
}

static void sb_guard(struct sb *sb)
{
	sb->guarded = true;
	if (!sb->hw_protect) {
		dev_warn(sb->dev, "guard off by no-hardware-protect: %u samples failed in %lld ms\n",
			 sb->failed, sb_failed_ms(sb));
		return;
	}
	dev_emerg(sb->dev, "guard: power off\n");
	hw_protection_trigger("AXP silent", SB_GUARD_MS);
}

static void sb_failed_sample(struct sb *sb, int err)
{
	if (!sb->failed++)
		sb->failed_since = ktime_get();
	if (!sb->silent && sb_failed_ms(sb) >= sb->silent_ms)
		sb_enter_silence(sb, err);
	if (!sb->guarded && sb_failed_ms(sb) >= sb->guard_ms)
		sb_guard(sb);
}

static void sb_good_sample(struct sb *sb)
{
	if (sb->silent)
		dev_info(sb->dev, "AXP answers again after %u failed samples, %lld ms\n",
			 sb->failed, sb_failed_ms(sb));
	sb->failed = 0;
	sb->silent = false;
	sb->guarded = false;
}

/*
 * The cells give current with the charger in: the charger is too weak
 * for the load. Only the mean of a full window counts, so one peak of the
 * load does not. At the full state the cells take a small current, so
 * the sign is enough.
 */
static bool sb_draining(const struct sb *sb, const struct sb_means *m)
{
	return sb->n_mean == SB_MEAN_SAMPLES && m->i_ua < 0;
}

/*
 * The status. No charger, or the cells give current: a discharge. With
 * the charger in and a small current of the charge, bit 6 of 0x01 tells
 * a charge from its end; the end with the voltage near the target is the
 * full state. Full holds until the charger goes, the PMIC charges again
 * or the cells give current, so the voltage that falls after the end
 * does not turn it back and forth.
 */
static int sb_status(struct sb *sb, bool supplied, const struct sb_sample *s,
		     const struct sb_means *m, int *status)
{
	bool charging;
	int ret;

	if (!supplied || sb_draining(sb, m)) {
		*status = POWER_SUPPLY_STATUS_DISCHARGING;
		return 0;
	}
	if (s->chrg_ma * 1000 >= (int)sb->full_current_ua) {
		*status = POWER_SUPPLY_STATUS_CHARGING;
		return 0;
	}

	ret = sb_read_charging(sb, &charging);
	if (ret)
		return ret;
	if (charging)
		*status = POWER_SUPPLY_STATUS_CHARGING;
	else if (sb->prev_status == POWER_SUPPLY_STATUS_FULL ||
		 s->v_mv * 1000 >= sb->full_uv)
		*status = POWER_SUPPLY_STATUS_FULL;
	else
		*status = POWER_SUPPLY_STATUS_NOT_CHARGING;
	return 0;
}

/* The means of the last samples; the charger in or out starts them again. */
static struct sb_means sb_add_mean(struct sb *sb, bool supplied,
				   const struct sb_sample *s)
{
	s64 sum_v = 0, sum_i = 0;
	int i;

	if (supplied != sb->prev_supplied)
		sb_mean_reset(sb);

	sb->v_uv[sb->pos_mean] = s->v_mv * 1000;
	sb->i_ua[sb->pos_mean] = (s->chrg_ma - s->dischrg_ma) * 1000;
	sb->pos_mean = (sb->pos_mean + 1) % SB_MEAN_SAMPLES;
	sb->n_mean = min(sb->n_mean + 1, SB_MEAN_SAMPLES);

	for (i = 0; i < sb->n_mean; i++) {
		sum_v += sb->v_uv[i];
		sum_i += sb->i_ua[i];
	}
	return (struct sb_means){
		.v_uv = div_s64(sum_v, sb->n_mean),
		.i_ua = div_s64(sum_i, sb->n_mean),
	};
}

/* The level does not rise on a discharge, nor fall in another status. */
static int sb_hold_level(struct sb *sb, int status, int level)
{
	if (sb->prev_level < 0)
		return level;
	return status == POWER_SUPPLY_STATUS_DISCHARGING ?
	       min(level, sb->prev_level) : max(level, sb->prev_level);
}

/* The copy for get_property; true when upower must hear of it. */
static bool sb_store(struct sb *sb, int status, int level,
		     const struct sb_sample *s)
{
	bool changed;

	guard(mutex)(&sb->lock);
	changed = !sb->snap.valid || sb->snap.silent ||
		  sb->snap.status != status || sb->snap.capacity != level;
	sb->snap.valid = true;
	sb->snap.silent = false;
	sb->snap.status = status;
	sb->snap.capacity = level;
	sb->snap.v_uv = s->v_mv * 1000;
	sb->snap.i_ua = (s->chrg_ma - s->dischrg_ma) * 1000;
	return changed;
}

/* One line of dmesg at an event, never at each sample. */
static void sb_tell_events(struct sb *sb, bool supplied, int status, int level,
			   const struct sb_sample *s, const struct sb_means *m)
{
	if (supplied != sb->prev_supplied)
		dev_info(sb->dev, "%s\n", supplied ? "charger in" : "charger out");
	if (status == POWER_SUPPLY_STATUS_FULL && sb->prev_status != status)
		dev_info(sb->dev, "full: %d mV at %d mA\n", s->v_mv, s->chrg_ma);
	if (supplied && status == POWER_SUPPLY_STATUS_DISCHARGING &&
	    sb->prev_status != status)
		dev_info(sb->dev, "discharge with the charger in: %d mV at %d mA\n",
			 m->v_uv / 1000, -m->i_ua / 1000);
	if (level == 0 && !sb->at_zero)
		dev_info(sb->dev, "level 0 %%: %d mV at %d mA\n",
			 m->v_uv / 1000, -m->i_ua / 1000);
}

static void sb_remember(struct sb *sb, bool supplied, int status, int level)
{
	sb->at_zero = level == 0;
	sb->prev_supplied = supplied;
	sb->prev_status = status;
	sb->prev_level = level;
}

static void sb_take_sample(struct sb *sb, const struct sb_sample *s)
{
	struct sb_means m;
	bool supplied, changed;
	int status, level;

	supplied = power_supply_am_i_supplied(sb->psy) > 0;
	m = sb_add_mean(sb, supplied, s);
	if (sb_status(sb, supplied, s, &m, &status)) {
		dev_err_ratelimited(sb->dev, "read of 0x01 failed\n");
		return;
	}

	level = sb_hold_level(sb, status, sb_level(sb, status, m.v_uv, m.i_ua));
	changed = sb_store(sb, status, level, s) ||
		  supplied != sb->prev_supplied;

	sb_tell_events(sb, supplied, status, level, s, &m);
	sb_remember(sb, supplied, status, level);
	if (changed)
		power_supply_changed(sb->psy);
}

void sb_work(struct work_struct *work)
{
	struct sb *sb = container_of(to_delayed_work(work), struct sb, work);
	struct sb_sample s;
	int err[3];

	sb_read_adc(sb, &s, err);
	if (sb_axp_silent(err)) {
		sb_failed_sample(sb, err[0]);
	} else if (err[0] || err[1] || err[2]) {
		dev_err_ratelimited(sb->dev, "ADC read failed: %d\n",
				    err[0] ?: err[1] ?: err[2]);
	} else {
		sb_good_sample(sb);
		sb_take_sample(sb, &s);
	}
	sb_schedule(sb);
}
