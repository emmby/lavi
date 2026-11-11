using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Lavi;

internal static class NativeMethods
{
    private const string Lib = NativeLoader.LibraryName;

    static NativeMethods()
    {
        NativeLoader.EnsureLoaded();
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void NativeActionCallback(
        IntPtr actionName,
        IntPtr context,
        IntPtr evt,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void NativeActionWithParamsCallback(
        IntPtr actionName,
        IntPtr context,
        IntPtr evt,
        IntPtr @params,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal delegate bool NativeGuardCallback(
        IntPtr guardName,
        IntPtr context,
        IntPtr evt,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal delegate bool NativeGuardWithParamsCallback(
        IntPtr guardName,
        IntPtr context,
        IntPtr evt,
        IntPtr @params,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate ulong NativeDelayCallback(
        IntPtr delayName,
        IntPtr context,
        IntPtr evt,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate ulong NativeClockWithDataCallback(IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void NativeInspectCallback(
        IntPtr inspectJson,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate UIntPtr NativeOutputResolverCallback(
        IntPtr statePath,
        IntPtr context,
        IntPtr evt,
        IntPtr outBuffer,
        UIntPtr outBufferLen,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void NativeInvokeStartCallback(
        IntPtr invokeId,
        IntPtr srcJson,
        IntPtr input,
        IntPtr context,
        IntPtr evt,
        IntPtr userData);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void NativeInvokeStopCallback(
        IntPtr invokeId,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr fsm_create(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string jsonConfig);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr fsm_create_with_input(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string jsonConfig,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string inputJson);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_start(IntPtr ptr);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void fsm_free(IntPtr ptr);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_matches(
        IntPtr ptr,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statePath);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_can(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string evt);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_has_tag(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string tag);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_is_done(IntPtr ctx);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_stop(IntPtr ctx);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_send_event(
        IntPtr ptr,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string eventName);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_send_json_event(
        IntPtr ptr,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string jsonEvent);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_get_state(
        IntPtr ctx,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_value_is_object(IntPtr val);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_value_is_string(IntPtr val);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_value_is_number(IntPtr val);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_value_is_bool(IntPtr val);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern UIntPtr fsm_value_get_string(
        IntPtr val,
        byte[]? buf,
        UIntPtr bufLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern double fsm_value_get_number(IntPtr val);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_value_get_bool(IntPtr val);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr fsm_value_get_property(
        IntPtr val,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string key);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_action(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string actionName,
        NativeActionCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_action_with_params(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string actionName,
        NativeActionWithParamsCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_guard(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string guardName,
        NativeGuardCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_guard_with_params(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string guardName,
        NativeGuardWithParamsCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_delay(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string delayName,
        NativeDelayCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_output_resolver(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statePath,
        NativeOutputResolverCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_invoke_callbacks(
        IntPtr ctx,
        NativeInvokeStartCallback? onStart,
        NativeInvokeStopCallback? onStop,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_clock_with_data(
        IntPtr ctx,
        NativeClockWithDataCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_inspector(
        IntPtr ctx,
        NativeInspectCallback callback,
        IntPtr userData);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool fsm_get_next_deadline(
        IntPtr ptr,
        out ulong outDeadline);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr fsm_simulated_clock_create(ulong initialMs);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void fsm_simulated_clock_free(IntPtr clock);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_register_simulated_clock(
        IntPtr ctx,
        IntPtr clock);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern ulong fsm_simulated_clock_now(IntPtr clock);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_simulated_clock_set(
        IntPtr clock,
        ulong targetMs);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_simulated_clock_increment(
        IntPtr clock,
        ulong dtMs);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_tick(IntPtr ptr);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_get_snapshot_json(
        IntPtr ctx,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_restore_snapshot_json(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string jsonStr);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_get_output_json(
        IntPtr ctx,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_get_final_leaf_output_json(
        IntPtr ctx,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_initial_transition_json(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? inputJson,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_transition_pure_json(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string snapshotJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string eventJson,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_get_meta_json(
        IntPtr ctx,
        byte[]? buffer,
        UIntPtr bufferLen);

    [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int fsm_get_state_meta_json(
        IntPtr ctx,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string statePath,
        byte[]? buffer,
        UIntPtr bufferLen);

    internal static string ReadBufferString(Func<byte[]?, UIntPtr, int> query)
    {
        var required = query(null, UIntPtr.Zero);
        if (required <= 1)
        {
            return string.Empty;
        }

        var buffer = new byte[required];
        var written = query(buffer, (UIntPtr)buffer.Length);
        if (written <= 0)
        {
            return string.Empty;
        }

        return Encoding.UTF8.GetString(buffer, 0, written);
    }
}
