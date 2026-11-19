// SPDX-License-Identifier: GPL-2.0
/*
 * axp22x_soft_battery: the battery of an AXP22x by a model of the device
 * tree, in place of axp20x_battery.
 *
 * The driver binds to the cell battery-power-supply of the MFD axp20x.
 * From the cell it takes what is of the hardware: the regmap of the PMIC,
 * the IIO channels of axp20x_adc, the supply of the charger. The values
 * are in a node of its own (see dt.c).
 */

#include <linux/device.h>
#include <linux/iio/consumer.h>
#include <linux/mod_devicetable.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/pm.h>
#include <linux/power_supply.h>
#include <linux/regmap.h>

#include "sb.h"

/* axp20x_battery built in binds first; a blacklist cannot stop it. */
#if IS_BUILTIN(CONFIG_BATTERY_AXP20X)
#error "CONFIG_BATTERY_AXP20X=y: axp20x_battery must be a module (m) or off (n)"
#endif

/*
 * The map of the channels is made by axp20x_adc at its probe; until then
 * the lookup gives -ENODEV, and the probe waits for it, as
 * axp20x_battery.c does.
 */
static int sb_channel_error(struct iio_channel *chan)
{
	return PTR_ERR(chan) == -ENODEV ? -EPROBE_DEFER : PTR_ERR(chan);
}

static int sb_get_channel(struct sb *sb, const char *name,
			  struct iio_channel **chan)
{
	*chan = devm_iio_channel_get(sb->dev, name);
	if (IS_ERR(*chan))
		return dev_err_probe(sb->dev, sb_channel_error(*chan),
				     "no IIO %s\n", name);
	return 0;
}

static int sb_get_hardware(struct sb *sb)
{
	sb->regmap = dev_get_regmap(sb->dev->parent, NULL);
	if (!sb->regmap)
		return dev_err_probe(sb->dev, -ENODEV, "no regmap of the PMIC\n");

	return sb_get_channel(sb, "batt_v", &sb->batt_v) ?:
	       sb_get_channel(sb, "batt_chrg_i", &sb->batt_chrg_i) ?:
	       sb_get_channel(sb, "batt_dischrg_i", &sb->batt_dischrg_i);
}

static void sb_put_node(void *data)
{
	of_node_put(data);
}

/*
 * The end of the work, before the power supply goes: no more runs, and
 * external_power_changed() can queue none.
 */
static void sb_stop(void *data)
{
	struct sb *sb = data;

	disable_delayed_work_sync(&sb->work);
	dev_info(sb->dev, "removed\n");
}

/*
 * The checks of the device tree, the set up of the PMIC and the line
 * "bound" run in the init of the power supply (props.c). When it fails,
 * a run may be queued already by external_power_changed().
 */
static int sb_register(struct sb *sb)
{
	struct power_supply_config cfg = {
		.fwnode = of_fwnode_handle(sb->np),
		.drv_data = sb,
	};
	struct power_supply *psy;

	psy = devm_power_supply_register(sb->dev, &sb_desc, &cfg);
	if (!IS_ERR(psy))
		return 0;
	disable_delayed_work_sync(&sb->work);
	return dev_err_probe(sb->dev, PTR_ERR(psy), "no power supply\n");
}

static int sb_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct sb *sb;
	int ret;

	sb = devm_kzalloc(dev, sizeof(*sb), GFP_KERNEL);
	if (!sb)
		return -ENOMEM;
	sb->dev = dev;
	sb->prev_level = -1;
	sb->prev_status = POWER_SUPPLY_STATUS_UNKNOWN;
	INIT_DELAYED_WORK(&sb->work, sb_work);
	ret = devm_mutex_init(dev, &sb->lock);
	if (ret)
		return ret;

	ret = sb_get_hardware(sb);
	if (ret)
		return ret;

	ret = sb_find_node(sb);
	if (sb->np && devm_add_action_or_reset(dev, sb_put_node, sb->np))
		return -ENOMEM;
	if (ret)
		return ret;

	ret = sb_register(sb);
	if (ret)
		return ret;
	ret = devm_add_action_or_reset(dev, sb_stop, sb);
	if (ret)
		return ret;

	platform_set_drvdata(pdev, sb);
	sb_sample_now(sb);
	return 0;
}

static int sb_suspend(struct device *dev)
{
	struct sb *sb = dev_get_drvdata(dev);

	cancel_delayed_work_sync(&sb->work);
	dev_info(dev, "suspend\n");
	return 0;
}

static int sb_resume(struct device *dev)
{
	struct sb *sb = dev_get_drvdata(dev);

	sb_mean_reset(sb);
	dev_info(dev, "resume\n");
	sb_sample_now(sb);
	return 0;
}

static DEFINE_SIMPLE_DEV_PM_OPS(sb_pm, sb_suspend, sb_resume);

static const struct of_device_id sb_of_match[] = {
	{ .compatible = "x-powers,axp221-battery-power-supply" },
	{ }
};
MODULE_DEVICE_TABLE(of, sb_of_match);

/*
 * The name is not the one of the node "axp22x-soft-battery": the platform
 * bus also matches a device by the name of its driver, and that node is a
 * device of its own, with no driver.
 */
static struct platform_driver sb_driver = {
	.probe = sb_probe,
	.driver = {
		.name = KBUILD_MODNAME,
		.of_match_table = sb_of_match,
		.pm = pm_sleep_ptr(&sb_pm),
	},
};
module_platform_driver(sb_driver);

MODULE_DESCRIPTION("AXP22x battery by a model of the device tree");
MODULE_LICENSE("GPL");
MODULE_IMPORT_NS("IIO_CONSUMER");
