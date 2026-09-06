#define _POSIX_C_SOURCE 200809L

/* Streamed test input for qualification on the active physical display. */

#include <errno.h>
#include <fcntl.h>
#include <linux/input-event-codes.h>
#include <linux/uinput.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

static void
fatal(const char *message)
{
	perror(message);
	exit(1);
}

static void
emit(int descriptor, unsigned short type, unsigned short code, int value)
{
	struct input_event event = { 0 };

	gettimeofday(&event.time, NULL);
	event.type = type;
	event.code = code;
	event.value = value;
	if (write(descriptor, &event, sizeof event) != sizeof event)
		fatal("write /dev/uinput");
}

static int
create_device(void)
{
	struct uinput_setup setup = { 0 };
	struct uinput_abs_setup x_axis = { 0 };
	struct uinput_abs_setup y_axis = { 0 };
	struct timespec ready_delay = { .tv_sec = 0, .tv_nsec = 500000000 };
	int descriptor = open("/dev/uinput", O_WRONLY | O_NONBLOCK);

	if (descriptor < 0)
		fatal("open /dev/uinput");
	if (ioctl(descriptor, UI_SET_EVBIT, EV_SYN) < 0 ||
	    ioctl(descriptor, UI_SET_EVBIT, EV_KEY) < 0 ||
	    ioctl(descriptor, UI_SET_KEYBIT, BTN_LEFT) < 0 ||
	    ioctl(descriptor, UI_SET_KEYBIT, KEY_X) < 0 ||
	    ioctl(descriptor, UI_SET_EVBIT, EV_REL) < 0 ||
	    ioctl(descriptor, UI_SET_RELBIT, REL_X) < 0 ||
	    ioctl(descriptor, UI_SET_RELBIT, REL_Y) < 0 ||
	    ioctl(descriptor, UI_SET_RELBIT, REL_WHEEL) < 0 ||
	    ioctl(descriptor, UI_SET_RELBIT, REL_HWHEEL) < 0 ||
	    ioctl(descriptor, UI_SET_EVBIT, EV_ABS) < 0 ||
	    ioctl(descriptor, UI_SET_ABSBIT, ABS_X) < 0 ||
	    ioctl(descriptor, UI_SET_ABSBIT, ABS_Y) < 0)
		fatal("configure /dev/uinput capabilities");

	setup.id.bustype = BUS_USB;
	setup.id.vendor = 0x1209;
	setup.id.product = 0x4d4d;
	strcpy(setup.name, "Mineral Qualification Input");
	if (ioctl(descriptor, UI_DEV_SETUP, &setup) < 0)
		fatal("UI_DEV_SETUP");
	x_axis.code = ABS_X;
	x_axis.absinfo.maximum = 10000;
	y_axis.code = ABS_Y;
	y_axis.absinfo.maximum = 10000;
	if (ioctl(descriptor, UI_ABS_SETUP, &x_axis) < 0 ||
	    ioctl(descriptor, UI_ABS_SETUP, &y_axis) < 0 ||
	    ioctl(descriptor, UI_DEV_CREATE) < 0)
		fatal("create uinput device");
	nanosleep(&ready_delay, NULL);
	return descriptor;
}

int
main(void)
{
	char operation[16];
	int first;
	int second;
	int descriptor = create_device();

	while (scanf("%15s %d %d", operation, &first, &second) == 3) {
		if (strcmp(operation, "move") == 0) {
			/* Absolute coordinates are normalized to the 0..10000 range. The
			 * relative axes above are advertised for conventional mouse
			 * classification but are not used for deterministic positioning. */
			emit(descriptor, EV_ABS, ABS_X, first);
			emit(descriptor, EV_ABS, ABS_Y, second);
		} else if (strcmp(operation, "button") == 0) {
			emit(descriptor, EV_KEY, (unsigned short)first, second);
		} else if (strcmp(operation, "key") == 0) {
			emit(descriptor, EV_KEY, (unsigned short)first, second);
		} else if (strcmp(operation, "scroll") == 0) {
			if (first != 0)
				emit(descriptor, EV_REL, REL_HWHEEL, first < 0 ? -1 : 1);
			if (second != 0)
				emit(descriptor, EV_REL, REL_WHEEL, second < 0 ? -1 : 1);
		} else {
			fprintf(stderr, "invalid operation: %s\n", operation);
			ioctl(descriptor, UI_DEV_DESTROY);
			close(descriptor);
			return 2;
		}
		emit(descriptor, EV_SYN, SYN_REPORT, 0);
	}
	if (ioctl(descriptor, UI_DEV_DESTROY) < 0 && errno != ENODEV)
		fatal("UI_DEV_DESTROY");
	close(descriptor);
	return 0;
}
