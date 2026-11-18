#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <node_api.h>

#define CHECK_NAPI(call) do { if ((call) != napi_ok) { napi_throw_error(env, NULL, "diagnostic Win32 export failed"); return NULL; } } while (0)

static napi_value enable_wer(napi_env env, napi_callback_info info) {
  (void)info;
  const DWORD before = GetErrorMode();
  SetErrorMode(before & ~(DWORD)SEM_NOGPFAULTERRORBOX);
  const DWORD after = GetErrorMode();
  napi_value result, old_value, new_value;
  CHECK_NAPI(napi_create_object(env, &result));
  CHECK_NAPI(napi_create_uint32(env, before, &old_value));
  CHECK_NAPI(napi_create_uint32(env, after, &new_value));
  CHECK_NAPI(napi_set_named_property(env, result, "before", old_value));
  CHECK_NAPI(napi_set_named_property(env, result, "after", new_value));
  return result;
}

NAPI_MODULE_INIT() {
  napi_value enable;
  CHECK_NAPI(napi_create_function(env, "enableWer", NAPI_AUTO_LENGTH, enable_wer, NULL, &enable));
  CHECK_NAPI(napi_set_named_property(env, exports, "enableWer", enable));
  return exports;
}
