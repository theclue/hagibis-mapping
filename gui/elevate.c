#include <Security/Authorization.h>
#include <Security/AuthorizationTags.h>
#include <stddef.h>

int hagibis_elevate(const char *path) {
    if (!path) return -1;
    AuthorizationRef auth = NULL;
    if (AuthorizationCreate(NULL, kAuthorizationEmptyEnvironment,
                            kAuthorizationFlagDefaults, &auth) != errAuthorizationSuccess)
        return -1;
    AuthorizationItem item = { kAuthorizationRightExecute, 0, NULL, 0 };
    AuthorizationRights rights = { 1, &item };
    if (AuthorizationCopyRights(auth, &rights, kAuthorizationEmptyEnvironment,
                                kAuthorizationFlagDefaults | kAuthorizationFlagInteractionAllowed
                                | kAuthorizationFlagPreAuthorize | kAuthorizationFlagExtendRights, NULL)
        != errAuthorizationSuccess) { AuthorizationFree(auth, kAuthorizationFlagDefaults); return -1; }
    char *args[] = { (char *)"OverrideHub", (char *)"--elevated", NULL };
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    OSStatus s = AuthorizationExecuteWithPrivileges(auth, path, kAuthorizationFlagDefaults, args, NULL);
#pragma clang diagnostic pop
    AuthorizationFree(auth, kAuthorizationFlagDefaults);
    return (s == errAuthorizationSuccess) ? 0 : -1;
}
