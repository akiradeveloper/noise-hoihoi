// SPDX-License-Identifier: MIT
// The only IREE ABI boundary. Build against the pinned runtime in build-iree.sh.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "iree/runtime/api.h"
#include "iree/async/util/proactor_pool.h"
#include "iree/hal/drivers/vulkan/api.h"
#include "iree/hal/drivers/local_sync/sync_driver.h"
#include "iree/hal/local/loaders/embedded_elf_loader.h"

#ifdef _WIN32
#define API __declspec(dllexport)
#else
#define API __attribute__((visibility("default")))
#endif
API uint32_t noise_iree_abi(void) { return 1; }
#define SPEC_BYTES (962 * sizeof(float))
#define STATE_BYTES (90228 * sizeof(float))
#define TRY(expr) do { status = (expr); if (!iree_status_is_ok(status)) goto done; } while (0)

typedef struct noise_stream {
  iree_runtime_instance_t* instance;
  iree_hal_device_t* device;
  iree_runtime_session_t* session;
  iree_runtime_call_t call;
  int call_ready;
  iree_hal_buffer_view_t* state;
  iree_hal_buffer_t* staging;
  iree_hal_buffer_mapping_t mapping;
  int mapped;
  iree_hal_semaphore_t* semaphore;
  uint64_t sequence;
} noise_stream;

static int finish(iree_status_t status, char* error, size_t capacity) {
  if (iree_status_is_ok(status)) return 0;
  if (capacity) {
    iree_host_size_t length = 0;
    iree_status_format(status, capacity, error, &length);
    error[capacity - 1] = 0;
  }
  iree_status_ignore(status);
  return 1;
}
static iree_status_t driver_create(iree_hal_driver_t** driver) {
  iree_hal_vulkan_driver_options_t options;
  iree_hal_vulkan_driver_options_initialize(&options);
  return iree_hal_vulkan_driver_create(iree_make_cstring_view("vulkan"),
      &options, NULL, iree_allocator_system(), driver);
}

// Single-thread inline execution for CPU: no Vulkan loader or GPU required.
static iree_status_t cpu_driver_create(iree_hal_driver_t** driver) {
  iree_hal_executable_loader_t* loader = NULL;
  iree_hal_allocator_t* allocator = NULL;
  iree_status_t status;
  TRY(iree_hal_embedded_elf_loader_create(NULL, iree_allocator_system(), &loader));
  TRY(iree_hal_allocator_create_heap(iree_make_cstring_view("cpu"),
      iree_allocator_system(), iree_allocator_system(), &allocator));
  iree_hal_sync_device_params_t params;
  iree_hal_sync_device_params_initialize(&params);
  TRY(iree_hal_sync_driver_create(iree_make_cstring_view("local-sync"), &params,
      1, &loader, allocator, iree_allocator_system(), driver));
done:
  iree_hal_allocator_release(allocator);
  iree_hal_executable_loader_release(loader);
  return status;
}

// Callback strings are borrowed for the duration of the call. Only hardware GPUs.
typedef void (*device_callback)(void*, const char*, size_t, const char*, size_t, int);
API int noise_iree_devices(device_callback callback, void* user, char* error, size_t capacity) {
  iree_hal_driver_t* driver = NULL;
  iree_hal_device_info_t* infos = NULL;
  iree_host_size_t count = 0;
  iree_status_t status;
  TRY(driver_create(&driver));
  TRY(iree_hal_driver_query_available_devices(driver, iree_allocator_system(), &count, &infos));
  for (iree_host_size_t i = 0; i < count; ++i) {
    iree_string_builder_t builder;
    iree_string_builder_initialize(iree_allocator_system(), &builder);
    status = iree_hal_driver_dump_device_info(driver, infos[i].device_id, &builder);
    if (iree_status_is_ok(status)) {
      // Pinned Vulkan driver's diagnostic format; unknown/software types are excluded.
      const char* info = iree_string_builder_buffer(&builder);
      if (!info) info = "";
      int kind = strstr(info, "\ntype: integrated-gpu\n") ? 1 :
                 strstr(info, "\ntype: discrete-gpu\n") ? 2 : 0;
      // The summary name includes driver/version text; show only the hardware name.
      const char* name = strncmp(info, "name: ", 6) == 0 ? info + 6 : NULL;
      const char* end = name ? strchr(name, '\n') : NULL;
      if (kind && end) callback(user, infos[i].path.data, infos[i].path.size,
                                name, (size_t)(end - name), kind);
    }
    iree_string_builder_deinitialize(&builder);
    if (!iree_status_is_ok(status)) goto done;
  }
done:
  iree_allocator_free(iree_allocator_system(), infos);
  iree_hal_driver_release(driver);
  return finish(status, error, capacity);
}

API void noise_iree_destroy(noise_stream* s) {
  if (!s) return;
  if (s->call_ready) iree_runtime_call_deinitialize(&s->call);
  iree_hal_buffer_view_release(s->state);
  if (s->mapped) iree_status_ignore(iree_hal_buffer_unmap_range(&s->mapping));
  iree_hal_buffer_release(s->staging);
  iree_hal_semaphore_release(s->semaphore);
  iree_runtime_session_release(s->session);
  iree_hal_device_release(s->device);
  iree_runtime_instance_release(s->instance);
  free(s);
}

static iree_status_t upload(noise_stream* s, const float* data, int state,
                           iree_hal_buffer_view_t** view) {
  const iree_hal_dim_t spectrum_shape[] = {1, 1, 481, 2};
  const iree_hal_dim_t state_shape[] = {90228};
  return iree_hal_buffer_view_allocate_buffer_copy(s->device,
      iree_hal_device_allocator(s->device), state ? 1 : 4,
      state ? state_shape : spectrum_shape, IREE_HAL_ELEMENT_TYPE_FLOAT_32,
      IREE_HAL_ENCODING_TYPE_DENSE_ROW_MAJOR,
      (iree_hal_buffer_params_t){.type = IREE_HAL_MEMORY_TYPE_DEVICE_LOCAL,
        .access = IREE_HAL_MEMORY_ACCESS_ALL, .usage = IREE_HAL_BUFFER_USAGE_DEFAULT},
      iree_make_const_byte_span(data, state ? STATE_BYTES : SPEC_BYTES), view);
}

// Model bytes must remain valid until destroy; Rust embeds an aligned static blob.
API int noise_iree_create(const char* uri, const uint8_t* model, size_t model_size,
                          const float* state, noise_stream** result,
                          char* error, size_t capacity) {
  *result = NULL;
  noise_stream* s = calloc(1, sizeof(*s));
  if (!s) return finish(iree_status_from_code(IREE_STATUS_RESOURCE_EXHAUSTED), error, capacity);
  iree_hal_driver_t* driver = NULL;
  iree_status_t status;
  iree_async_proactor_pool_t* pool = NULL;
  iree_runtime_instance_options_t options;
  iree_runtime_instance_options_initialize(&options);
  TRY(iree_runtime_instance_create(&options, iree_allocator_system(), &s->instance));
  const int cpu = strcmp(uri, "cpu") == 0;
  TRY(cpu ? cpu_driver_create(&driver) : driver_create(&driver));
  TRY(iree_async_proactor_pool_create(1, NULL, iree_async_proactor_pool_options_default(),
      iree_allocator_system(), &pool));
  iree_hal_device_create_params_t params = iree_hal_device_create_params_default();
  params.proactor_pool = pool;
  TRY(iree_hal_driver_create_device_by_uri(driver, iree_make_cstring_view(cpu ? "local-sync://" : uri),
      &params, iree_allocator_system(), &s->device));
  iree_runtime_session_options_t session_options;
  iree_runtime_session_options_initialize(&session_options);
  TRY(iree_runtime_session_create_with_device(s->instance, &session_options,
      s->device, iree_allocator_system(), &s->session));
  TRY(iree_runtime_session_append_bytecode_module_from_memory(s->session,
      iree_make_const_byte_span(model, model_size), iree_allocator_null()));
  TRY(iree_runtime_call_initialize_by_name(s->session,
      iree_make_cstring_view("module.main_graph"), &s->call));
  s->call_ready = 1;
  TRY(upload(s, state, 1, &s->state));
  TRY(iree_hal_allocator_allocate_buffer(iree_hal_device_allocator(s->device),
      (iree_hal_buffer_params_t){
        .type = IREE_HAL_MEMORY_TYPE_HOST_LOCAL | IREE_HAL_MEMORY_TYPE_DEVICE_VISIBLE,
        .access = IREE_HAL_MEMORY_ACCESS_ALL,
        .usage = IREE_HAL_BUFFER_USAGE_TRANSFER_TARGET | IREE_HAL_BUFFER_USAGE_MAPPING_SCOPED},
      SPEC_BYTES, &s->staging));
  TRY(iree_hal_buffer_map_range(s->staging, IREE_HAL_MAPPING_MODE_SCOPED,
      IREE_HAL_MEMORY_ACCESS_READ, 0, SPEC_BYTES, &s->mapping));
  s->mapped = 1;
  TRY(iree_hal_semaphore_create(s->device, IREE_HAL_QUEUE_AFFINITY_ANY, 0,
      IREE_HAL_SEMAPHORE_FLAG_NONE, &s->semaphore));
  *result = s;
done:
  iree_async_proactor_pool_release(pool);
  iree_hal_driver_release(driver);
  if (!iree_status_is_ok(status)) noise_iree_destroy(s);
  return finish(status, error, capacity);
}

API int noise_iree_reset(noise_stream* s, const float* state, char* error, size_t capacity) {
  iree_hal_buffer_view_t* next = NULL;
  iree_status_t status = upload(s, state, 1, &next);
  if (iree_status_is_ok(status)) {
    iree_runtime_call_reset(&s->call);
    iree_hal_buffer_view_release(s->state);
    s->state = next;
  }
  return finish(status, error, capacity);
}

API int noise_iree_process(noise_stream* s, const float* input, float* output,
                           char* error, size_t capacity) {
  iree_hal_buffer_view_t *spec = NULL, *enhanced = NULL, *next = NULL;
  iree_status_t status;
  iree_runtime_call_reset(&s->call);
  TRY(upload(s, input, 0, &spec));
  TRY(iree_runtime_call_inputs_push_back_buffer_view(&s->call, spec));
  TRY(iree_runtime_call_inputs_push_back_buffer_view(&s->call, s->state));
  TRY(iree_runtime_call_invoke(&s->call, 0));
  TRY(iree_runtime_call_outputs_pop_front_buffer_view(&s->call, &enhanced));
  TRY(iree_runtime_call_outputs_pop_front_buffer_view(&s->call, &next));
  ++s->sequence;
  TRY(iree_hal_device_queue_copy(s->device, IREE_HAL_QUEUE_AFFINITY_ANY,
      iree_hal_semaphore_list_empty(),
      (iree_hal_semaphore_list_t){1, &s->semaphore, &s->sequence},
      iree_hal_buffer_view_buffer(enhanced), 0, s->staging, 0, SPEC_BYTES, 0));
  TRY(iree_hal_semaphore_wait(s->semaphore, s->sequence, iree_make_timeout_ms(5000), 0));
  TRY(iree_hal_buffer_mapping_invalidate_range(&s->mapping, 0, SPEC_BYTES));
  memcpy(output, s->mapping.contents.data, SPEC_BYTES);
  iree_hal_buffer_view_release(s->state);
  s->state = next;
  next = NULL;
done:
  iree_hal_buffer_view_release(next);
  iree_hal_buffer_view_release(enhanced);
  iree_hal_buffer_view_release(spec);
  return finish(status, error, capacity);
}
