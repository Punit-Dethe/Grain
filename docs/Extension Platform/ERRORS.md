# Grain extension host errors

Every rejected host-API promise throws a `GrainError`. Its `code` is stable;
`message`, `hint`, and `docs` are always present. Capability failures also carry
`capability`.

```ts
try {
  await grain.storage.get("key");
} catch (error) {
  const failure = error as GrainError;
  console.error(failure.code, failure.hint, failure.docs);
}
```

## E_CAPABILITY_DENIED

The call requires a capability the extension does not hold. Add the named
capability to `permissions` in `manifest.json`, reload, and let the user approve
the new permission.

## E_TIMEOUT

A bounded host operation exceeded its deadline. Retry or reduce the request.

## E_SESSION_BUSY

Another Grain or extension-owned recording is already active. Wait for that
session to stop; extension sessions are serialized and never queue or interrupt
the current recording.

## E_QUOTA

The extension's storage write would exceed its quota. Delete data the extension
no longer needs before retrying.

## E_RESPONSE_TOO_LARGE

A proxied network response exceeded the 2 MiB host limit. Request a smaller
representation or use a paginated endpoint.

## E_INVALID_MANIFEST

The requested contribution is absent or invalid in `manifest.json`. Correct the
declaration and reload the extension.

## E_INVALID_ARGUMENT

The call is missing a required field or contains a value of the wrong type.
Use the current generated `grain.d.ts` signature.

## E_NOT_IMPLEMENTED

The API name is reserved but this Grain build does not implement it yet.

## E_UNKNOWN_METHOD

The method is not part of the current Grain API. Rebuild against the current
SDK rather than sending raw host frames.

## E_UNAVAILABLE

The configured service or local model needed by the call is unavailable. Check
its Grain settings and retry.

## E_INTERNAL

Grain could not complete the operation because internal state, storage, or a
task failed. Retry once; if it persists, copy Extensions > Developer logs.
