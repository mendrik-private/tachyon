/* Command-line client for the private mineral_input_v1 control global. */

#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <wayland-client-core.h>
#include <wayland-client-protocol.h>

static const struct wl_message mineral_input_requests[] = {
	{ "move", "ii", NULL },
	{ "button", "uu", NULL },
	{ "key", "uu", NULL },
	{ "scroll", "ii", NULL },
};

static const struct wl_interface mineral_input_interface = {
	.name = "mineral_input_v1",
	.version = 1,
	.method_count = 4,
	.methods = mineral_input_requests,
	.event_count = 0,
	.events = NULL,
};

struct client_state {
	struct wl_display *display;
	struct wl_registry *registry;
	struct wl_proxy *input;
};

static void
registry_global(void *data, struct wl_registry *registry, uint32_t name,
		const char *interface, uint32_t version)
{
	struct client_state *state = data;

	(void)version;
	if (strcmp(interface, mineral_input_interface.name) == 0)
		state->input = (struct wl_proxy *)wl_registry_bind(
			registry, name, &mineral_input_interface, 1);
}

static void
registry_global_remove(void *data, struct wl_registry *registry, uint32_t name)
{
	(void)data;
	(void)registry;
	(void)name;
}

static const struct wl_registry_listener registry_listener = {
	.global = registry_global,
	.global_remove = registry_global_remove,
};

static int32_t
parse_int(const char *value)
{
	char *end = NULL;
	long result;

	errno = 0;
	result = strtol(value, &end, 10);
	if (errno || !end || *end != '\0' || result < INT32_MIN || result > INT32_MAX) {
		fprintf(stderr, "invalid integer: %s\n", value);
		exit(2);
	}
	return (int32_t)result;
}

static uint32_t
operation_code(const char *operation)
{
	if (strcmp(operation, "move") == 0)
		return 0;
	if (strcmp(operation, "button") == 0)
		return 1;
	if (strcmp(operation, "key") == 0)
		return 2;
	if (strcmp(operation, "scroll") == 0)
		return 3;
	fprintf(stderr, "invalid operation: %s\n", operation);
	exit(2);
}

static void
send_request(struct client_state *state, const char *operation,
	     const char *first_value, const char *second_value)
{
	int32_t first = parse_int(first_value);
	int32_t second = parse_int(second_value);

	wl_proxy_marshal(state->input, operation_code(operation), first, second);
}

int
main(int argc, char *argv[])
{
	struct client_state state = { 0 };
	bool stream;

	stream = argc == 2 && strcmp(argv[1], "stream") == 0;
	if (!stream && argc != 4) {
		fprintf(stderr,
			"usage: %s move|button|key|scroll VALUE VALUE | stream\n",
			argv[0]);
		return 2;
	}

	state.display = wl_display_connect(NULL);
	if (!state.display) {
		fprintf(stderr, "unable to connect to the Weston display\n");
		return 1;
	}
	state.registry = wl_display_get_registry(state.display);
	wl_registry_add_listener(state.registry, &registry_listener, &state);
	if (wl_display_roundtrip(state.display) < 0 || !state.input) {
		fprintf(stderr, "mineral_input_v1 is unavailable\n");
		return 1;
	}

	if (stream) {
		char operation[16];
		char first[32];
		char second[32];

		while (scanf("%15s %31s %31s", operation, first, second) == 3) {
			send_request(&state, operation, first, second);
			if (wl_display_flush(state.display) < 0) {
				fprintf(stderr, "input stream failed\n");
				return 1;
			}
		}
	} else {
		send_request(&state, argv[1], argv[2], argv[3]);
		if (wl_display_roundtrip(state.display) < 0) {
			fprintf(stderr, "input request failed\n");
			return 1;
		}
	}

	wl_proxy_destroy(state.input);
	wl_registry_destroy(state.registry);
	wl_display_disconnect(state.display);
	return 0;
}
