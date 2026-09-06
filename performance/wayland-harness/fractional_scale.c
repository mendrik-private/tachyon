#include <stdint.h>
#include <stdlib.h>

#include <wayland-server-core.h>

#include "fractional-scale-v1-server.h"

struct weston_compositor {
	struct wl_signal destroy_signal;
	_Bool shutting_down;
	struct wl_display *wl_display;
};

struct mineral_fractional_scale {
	uint32_t scale_120;
	struct wl_global *global;
};

struct mineral_fractional_surface {
	struct mineral_fractional_scale *state;
	struct wl_resource *resource;
	struct wl_event_source *timer;
};

static int
repeat_preferred_scale(void *data)
{
	struct mineral_fractional_surface *surface = data;
	struct wl_event_source *timer = surface->timer;

	surface->timer = NULL;
	wp_fractional_scale_v1_send_preferred_scale(
		surface->resource, surface->state->scale_120);
	wl_client_flush(wl_resource_get_client(surface->resource));
	wl_event_source_remove(timer);
	return 0;
}

static void
fractional_scale_resource_destroyed(struct wl_resource *resource)
{
	struct mineral_fractional_surface *surface =
		wl_resource_get_user_data(resource);

	if (surface->timer)
		wl_event_source_remove(surface->timer);
	free(surface);
}

static void
fractional_scale_destroy(struct wl_client *client, struct wl_resource *resource)
{
	(void)client;
	wl_resource_destroy(resource);
}

static const struct wp_fractional_scale_v1_interface fractional_scale_implementation = {
	.destroy = fractional_scale_destroy,
};

static void
manager_destroy(struct wl_client *client, struct wl_resource *resource)
{
	(void)client;
	wl_resource_destroy(resource);
}

static void
manager_get_fractional_scale(struct wl_client *client,
			     struct wl_resource *manager_resource,
			     uint32_t id,
			     struct wl_resource *surface)
{
	struct mineral_fractional_scale *state =
		wl_resource_get_user_data(manager_resource);
	struct mineral_fractional_surface *fractional_surface;
	struct wl_resource *resource;
	struct wl_event_loop *event_loop;

	(void)surface;
	resource = wl_resource_create(client, &wp_fractional_scale_v1_interface, 1, id);
	if (!resource) {
		wl_client_post_no_memory(client);
		return;
	}
	fractional_surface = calloc(1, sizeof *fractional_surface);
	if (!fractional_surface) {
		wl_resource_destroy(resource);
		wl_client_post_no_memory(client);
		return;
	}
	fractional_surface->state = state;
	fractional_surface->resource = resource;
	wl_resource_set_implementation(resource, &fractional_scale_implementation,
				       fractional_surface,
				       fractional_scale_resource_destroyed);
	wp_fractional_scale_v1_send_preferred_scale(resource, state->scale_120);
	wl_client_flush(client);
	event_loop = wl_display_get_event_loop(wl_client_get_display(client));
	fractional_surface->timer = wl_event_loop_add_timer(
		event_loop, repeat_preferred_scale, fractional_surface);
	if (fractional_surface->timer)
		wl_event_source_timer_update(fractional_surface->timer, 250);
}

static const struct wp_fractional_scale_manager_v1_interface manager_implementation = {
	.destroy = manager_destroy,
	.get_fractional_scale = manager_get_fractional_scale,
};

static void
bind_manager(struct wl_client *client, void *data, uint32_t version, uint32_t id)
{
	struct wl_resource *resource = wl_resource_create(
		client, &wp_fractional_scale_manager_v1_interface,
		version < 1 ? version : 1, id);
	if (!resource) {
		wl_client_post_no_memory(client);
		return;
	}
	wl_resource_set_implementation(resource, &manager_implementation, data, NULL);
}

__attribute__((visibility("default"))) int
wet_module_init(struct weston_compositor *compositor, int *argc, char *argv[])
{
	struct mineral_fractional_scale *state;
	const char *configured = getenv("MINERAL_WESTON_SCALE_120");
	char *end = NULL;
	unsigned long parsed = configured ? strtoul(configured, &end, 10) : 120;

	(void)argc;
	(void)argv;
	if ((configured && (!end || *end != '\0')) || parsed < 120 || parsed > 240)
		return -1;
	state = calloc(1, sizeof *state);
	if (!state)
		return -1;
	state->scale_120 = (uint32_t)parsed;
	state->global = wl_global_create(
		compositor->wl_display, &wp_fractional_scale_manager_v1_interface,
		1, state, bind_manager);
	return state->global ? 0 : -1;
}
