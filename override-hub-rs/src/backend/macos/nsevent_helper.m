// Helper: ObjC functions called from Rust to avoid objc_msgSend ABI issues.
#include <CoreGraphics/CoreGraphics.h>
#include <Foundation/Foundation.h>
#include <AppKit/NSEvent.h>
#include <AppKit/NSWorkspace.h>

#define NX_KEYTYPE_SOUND_UP     0
#define NX_KEYTYPE_SOUND_DOWN   1
#define NX_KEYTYPE_MUTE         7
#define NX_KEYTYPE_PLAY        16
#define NX_KEYTYPE_BRIGHTNESS_UP   2
#define NX_KEYTYPE_BRIGHTNESS_DOWN 3

#define NSEventTypeSystemDefined 14
#define NX_SUBTYPE_AUX_CONTROL_BUTTONS 8
#define NX_SUBTYPE_EJECT_KEY    10
#define NX_SUBTYPE_SLEEP        11
#define NX_SUBTYPE_RESTART      12
#define NX_SUBTYPE_SHUTDOWN     13

void hagibis_post_media_key(int key_code, int key_down) {
    @autoreleasepool {
        NSEvent *event = [NSEvent
            otherEventWithType:NSEventTypeSystemDefined
                      location:NSMakePoint(0, 0)
                 modifierFlags:(key_down ? 0xA00 : 0xB00)
                     timestamp:0
                  windowNumber:0
                       context:nil
                       subtype:NX_SUBTYPE_AUX_CONTROL_BUTTONS
                         data1:(key_code << 16) | ((key_down ? 0xA00 : 0xB00))
                         data2:-1];
        CGEventPost(kCGHIDEventTap, [event CGEvent]);
    }
}

void hagibis_post_system_event(int subtype, int data) {
    (void)data;
    @autoreleasepool {
        // Brightness uses the media-key pathway
        if (subtype == NX_KEYTYPE_BRIGHTNESS_UP || subtype == NX_KEYTYPE_BRIGHTNESS_DOWN) {
            hagibis_post_media_key(subtype, 1);
            hagibis_post_media_key(subtype, 0);
            return;
        }

        // Sleep / Restart / Shutdown / Eject
        NSEvent *event = [NSEvent
            otherEventWithType:NSEventTypeSystemDefined
                      location:NSMakePoint(0, 0)
                 modifierFlags:0
                     timestamp:0
                  windowNumber:0
                       context:nil
                       subtype:subtype
                         data1:0
                         data2:-1];
        CGEventPost(kCGHIDEventTap, [event CGEvent]);
    }
}

int hagibis_focused_app(char *bundle_id_out, int bundle_id_cap,
                         char *name_out, int name_cap) {
    @autoreleasepool {
        NSRunningApplication *app = [[NSWorkspace sharedWorkspace] frontmostApplication];
        if (!app) return 0;

        NSString *bid = [app bundleIdentifier];
        NSString *nm  = [app localizedName];

        if (bid) {
            snprintf(bundle_id_out, bundle_id_cap, "%s", [bid UTF8String]);
        } else {
            bundle_id_out[0] = '\0';
        }
        if (nm) {
            snprintf(name_out, name_cap, "%s", [nm UTF8String]);
        } else {
            name_out[0] = '\0';
        }
        return 1;
    }
}
