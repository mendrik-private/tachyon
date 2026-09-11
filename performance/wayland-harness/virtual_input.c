/* Weston 14 test-only virtual seat and deterministic input bridge. */

#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

#include <linux/input-event-codes.h>
#include <wayland-server-core.h>
#include <wayland-server-protocol.h>

#define TACHYON_SEAT_STORAGE_BYTES 4096

struct weston_compositor {
	struct wl_signal destroy_signal;
	bool shutting_down;
	struct wl_display *wl_display;
};

struct weston_seat;
struct xkb_keymap;
struct weston_pointer_axis_event;

struct weston_coord {
	double x;
	double y;
};

struct weston_coord_global {
	struct weston_coord c;
};

extern void weston_seat_init(struct weston_seat *seat,
			     struct weston_compositor *compositor,
			     const char *seat_name);
extern int weston_seat_init_pointer(struct weston_seat *seat);
extern int weston_seat_init_keyboard(struct weston_seat *seat,
				     struct xkb_keymap *keymap);
extern void notify_motion_absolute(struct weston_seat *seat,
				   const struct timespec *time,
				   struct weston_coord_global pos);
extern void notify_button(struct weston_seat *seat,
			  const struct timespec *time,
			  int32_t button,
			  enum wl_pointer_button_state state);
extern void notify_pointer_frame(struct weston_seat *seat);
extern void notify_axis(struct weston_seat *seat,
			const struct timespec *time,
			struct weston_pointer_axis_event *event);
extern void notify_axis_source(struct weston_seat *seat, uint32_t source);
extern void notify_key(struct weston_seat *seat,
		       const struct timespec *time,
		       uint32_t key,
		       enum wl_keyboard_key_state state,
		       int update_state);
extern void weston_compositor_get_time(struct timespec *time);

enum weston_key_state_update {
	STATE_UPDATE_AUTOMATIC,
};

struct weston_pointer_axis_event {
	uint32_t axis;
	double value;
	bool has_discrete;
	int32_t discrete;
};

struct tachyon_input {
	struct weston_seat *seat;
	struct wl_global *global;
};

struct tachyon_input_implementation {
	void (*move)(struct wl_client *client,
		     struct wl_resource *resource,
		     int32_t x,
		     int32_t y);
	void (*button)(struct wl_client *client,
		       struct wl_resource *resource,
		       uint32_t button,
		       uint32_t state);
	void (*key)(struct wl_client *client,
		    struct wl_resource *resource,
		    uint32_t key,
		    uint32_t state);
	void (*scroll)(struct wl_client *client,
		       struct wl_resource *resource,
		       int32_t horizontal_milli,
		       int32_t vertical_milli);
	void (*continuous_scroll)(struct wl_client *client,
				 struct wl_resource *resource,
				 int32_t horizontal_milli,
				 int32_t vertical_milli);
};

static const struct wl_message tachyon_input_requests[] = {
	{ "move", "ii", NULL },
	{ "button", "uu", NULL },
	{ "key", "uu", NULL },
	{ "scroll", "ii", NULL },
	{ "continuous_scroll", "ii", NULL },
};

static const struct wl_interface tachyon_input_interface = {
	.name = "tachyon_input_v1",
	.version = 1,
	.method_count = 5,
	.methods = tachyon_input_requests,
	.event_count = 0,
	.events = NULL,
};

static void
handle_move(struct wl_client *client, struct wl_resource *resource,
	    int32_t x, int32_t y)
{
	struct tachyon_input *input = wl_resource_get_user_data(resource);
	struct weston_coord_global position = { .c = { x, y } };
	struct timespec time;

	(void)client;
	weston_compositor_get_time(&time);
	notify_motion_absolute(input->seat, &time, position);
	notify_pointer_frame(input->seat);
}

static void
handle_button(struct wl_client *client, struct wl_resource *resource,
	      uint32_t button, uint32_t state)
{
	struct tachyon_input *input = wl_resource_get_user_data(resource);
	struct timespec time;

	(void)client;
	if (state > WL_POINTER_BUTTON_STATE_PRESSED)
		return;
	weston_compositor_get_time(&time);
	notify_button(input->seat, &time, (int32_t)button, state);
	notify_pointer_frame(input->seat);
}

static void
handle_key(struct wl_client *client, struct wl_resource *resource,
	   uint32_t key, uint32_t state)
{
	struct tachyon_input *input = wl_resource_get_user_data(resource);
	struct timespec time;

	(void)client;
	if (state > WL_KEYBOARD_KEY_STATE_PRESSED)
		return;
	weston_compositor_get_time(&time);
	notify_key(input->seat, &time, key, state, STATE_UPDATE_AUTOMATIC);
}

static void
send_scroll(struct wl_client *client, struct wl_resource *resource,
	    int32_t horizontal_milli, int32_t vertical_milli, bool continuous)
{
	struct tachyon_input *input = wl_resource_get_user_data(resource);
	struct weston_pointer_axis_event event = { 0 };
	struct timespec time;

	(void)client;
	weston_compositor_get_time(&time);
	notify_axis_source(input->seat, continuous ? WL_POINTER_AXIS_SOURCE_FINGER : WL_POINTER_AXIS_SOURCE_WHEEL);
	if (vertical_milli != 0) {
		event.axis = WL_POINTER_AXIS_VERTICAL_SCROLL;
		event.value = vertical_milli / 1000.0;
		event.has_discrete = !continuous;
		event.discrete = vertical_milli < 0 ? -1 : 1;
		notify_axis(input->seat, &time, &event);
	}
	if (horizontal_milli != 0) {
		event.axis = WL_POINTER_AXIS_HORIZONTAL_SCROLL;
		event.value = horizontal_milli / 1000.0;
		event.has_discrete = !continuous;
		event.discrete = horizontal_milli < 0 ? -1 : 1;
		notify_axis(input->seat, &time, &event);
	}
	notify_pointer_frame(input->seat);
}

static void
handle_scroll(struct wl_client *client, struct wl_resource *resource,
	      int32_t horizontal_milli, int32_t vertical_milli)
{
	send_scroll(client, resource, horizontal_milli, vertical_milli, false);
}

static void
handle_continuous_scroll(struct wl_client *client, struct wl_resource *resource,
			 int32_t horizontal_milli, int32_t vertical_milli)
{
	send_scroll(client, resource, horizontal_milli, vertical_milli, true);
}

static const struct tachyon_input_implementation input_implementation = {
	.move = handle_move,
	.button = handle_button,
	.key = handle_key,
	.scroll = handle_scroll,
	.continuous_scroll = handle_continuous_scroll,
};

static void
bind_input(struct wl_client *client, void *data, uint32_t version, uint32_t id)
{
	struct wl_resource *resource = wl_resource_create(
		client, &tachyon_input_interface, version < 1 ? version : 1, id);
	if (!resource) {
		wl_client_post_no_memory(client);
		return;
	}
	wl_resource_set_implementation(resource, &input_implementation, data, NULL);
}

__attribute__((visibility("default"))) int
wet_module_init(struct weston_compositor *compositor, int *argc, char *argv[])
{
	struct tachyon_input *input;

	(void)argc;
	(void)argv;
	input = calloc(1, sizeof *input);
	if (!input)
		return -1;
	input->seat = calloc(1, TACHYON_SEAT_STORAGE_BYTES);
	if (!input->seat)
		return -1;

	weston_seat_init(input->seat, compositor, "tachyon-test");
	if (weston_seat_init_pointer(input->seat) < 0)
		return -1;
	if (weston_seat_init_keyboard(input->seat, NULL) < 0)
		return -1;

	input->global = wl_global_create(compositor->wl_display,
					 &tachyon_input_interface, 1,
					 input, bind_input);
	return input->global ? 0 : -1;
}
