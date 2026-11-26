// Gives Spotlight and Finder an X3F's capture details from the shared capture facts.
// macOS indexes files through importer plug-ins like this one; Core Spotlight import
// extensions do not run for files on macOS. The plug-in boilerplate follows Apple's
// Spotlight Importer Programming Guide.
#include <CoreFoundation/CFPlugInCOM.h>
#include <CoreFoundation/CoreFoundation.h>
#include <CoreServices/CoreServices.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

#include "FoveonicQuickLook.h"

// cargo xtask bundle defines the factory ID it also writes into the Info.plist.
#ifndef FOVEONIC_SPOTLIGHT_FACTORY
#error "FOVEONIC_SPOTLIGHT_FACTORY must name the plug-in factory UUID"
#endif

typedef struct {
    MDImporterInterfaceStruct *interface;
    CFUUIDRef factory;
    UInt32 references;
} Importer;

static void set_text(CFMutableDictionaryRef attributes, CFStringRef key, const char *text) {
    if (text[0] == '\0') return;
    CFStringRef value = CFStringCreateWithCString(kCFAllocatorDefault, text, kCFStringEncodingUTF8);
    if (value) {
        CFDictionarySetValue(attributes, key, value);
        CFRelease(value);
    }
}

static void set_number(CFMutableDictionaryRef attributes, CFStringRef key, CFNumberType type,
                       const void *value) {
    CFNumberRef number = CFNumberCreate(kCFAllocatorDefault, type, value);
    if (!number) return;
    CFDictionarySetValue(attributes, key, number);
    CFRelease(number);
}

static void set_real(CFMutableDictionaryRef attributes, CFStringRef key, double value) {
    if (isfinite(value)) set_number(attributes, key, kCFNumberDoubleType, &value);
}

static void set_count(CFMutableDictionaryRef attributes, CFStringRef key, SInt64 value) {
    if (value > 0) set_number(attributes, key, kCFNumberSInt64Type, &value);
}

/// The camera records its wall-clock time without a zone, so read it in the local zone.
/// The digits are Gregorian whatever calendar the user has chosen.
static void set_taken(CFMutableDictionaryRef attributes, const char *taken) {
    int year, month, day, hour, minute, second;
    if (sscanf(taken, "%d:%d:%d %d:%d:%d", &year, &month, &day, &hour, &minute, &second) != 6) {
        return;
    }
    CFCalendarRef calendar = CFCalendarCreateWithIdentifier(kCFAllocatorDefault, kCFGregorianCalendar);
    if (!calendar) return;
    CFAbsoluteTime time;
    if (CFCalendarComposeAbsoluteTime(calendar, &time, "yMdHms", year, month, day, hour, minute,
                                      second)) {
        CFDateRef date = CFDateCreate(kCFAllocatorDefault, time);
        if (date) {
            CFDictionarySetValue(attributes, kMDItemContentCreationDate, date);
            CFRelease(date);
        }
    }
    CFRelease(calendar);
}

static const char *program_name(uint16_t code) {
    switch (code) {
    case 1: return "Manual";
    case 2: return "Program";
    case 3: return "Aperture priority";
    case 4: return "Shutter priority";
    default: return "";
    }
}

static const char *metering_name(uint16_t code) {
    switch (code) {
    case 1: return "Average";
    case 2: return "Center-weighted";
    case 5: return "Multi-segment";
    default: return "";
    }
}

static Boolean GetMetadataForFile(void *this, CFMutableDictionaryRef attributes,
                                  CFStringRef contentType, CFStringRef path) {
    (void)this;
    (void)contentType;
    char file[PATH_MAX];
    if (!CFStringGetFileSystemRepresentation(path, file, sizeof file)) return FALSE;
    FoveonicCapture capture;
    char error[512];
    if (!foveonic_ql_capture(file, &capture, error, sizeof error)) return FALSE;
    set_text(attributes, kMDItemAcquisitionMake, capture.make);
    set_text(attributes, kMDItemAcquisitionModel, capture.model);
    set_text(attributes, kMDItemLensModel, capture.lens);
    set_real(attributes, kMDItemFocalLength, capture.focal_length);
    set_real(attributes, kMDItemFNumber, capture.f_number);
    set_real(attributes, kMDItemExposureTimeSeconds, capture.exposure_seconds);
    // Spotlight has no standard key for exposure compensation; schema.xml declares this one.
    set_real(attributes, CFSTR("org_foveonic_exposure_bias"), capture.exposure_bias);
    set_count(attributes, kMDItemISOSpeed, capture.iso);
    set_count(attributes, kMDItemFocalLength35mm, capture.focal_length_35mm);
    set_text(attributes, kMDItemExposureProgram, program_name(capture.exposure_program));
    set_text(attributes, kMDItemMeteringMode, metering_name(capture.metering));
    set_count(attributes, kMDItemPixelWidth, capture.width);
    set_count(attributes, kMDItemPixelHeight, capture.height);
    set_taken(attributes, capture.taken);
    return TRUE;
}

// CFPlugIn COM boilerplate.
static HRESULT QueryInterface(void *this, REFIID iid, LPVOID *interface);
static ULONG AddRef(void *this);
static ULONG Release(void *this);

static MDImporterInterfaceStruct interface_table = {
    NULL, QueryInterface, AddRef, Release, GetMetadataForFile,
};

static Importer *create(CFUUIDRef factory) {
    Importer *importer = malloc(sizeof *importer);
    if (!importer) return NULL;
    importer->interface = &interface_table;
    importer->factory = CFRetain(factory);
    CFPlugInAddInstanceForFactory(factory);
    importer->references = 1;
    return importer;
}

static HRESULT QueryInterface(void *this, REFIID iid, LPVOID *interface) {
    CFUUIDRef requested = CFUUIDCreateFromUUIDBytes(kCFAllocatorDefault, iid);
    Boolean supported = CFEqual(requested, kMDImporterInterfaceID) ||
                        CFEqual(requested, IUnknownUUID);
    CFRelease(requested);
    if (!supported) {
        *interface = NULL;
        return E_NOINTERFACE;
    }
    AddRef(this);
    *interface = this;
    return S_OK;
}

static ULONG AddRef(void *this) {
    return ++((Importer *)this)->references;
}

static ULONG Release(void *this) {
    Importer *importer = this;
    if (--importer->references > 0) return importer->references;
    CFPlugInRemoveInstanceForFactory(importer->factory);
    CFRelease(importer->factory);
    free(importer);
    return 0;
}

void *FoveonicSpotlightFactory(CFAllocatorRef allocator, CFUUIDRef type) {
    (void)allocator;
    if (!CFEqual(type, kMDImporterTypeID)) return NULL;
    CFUUIDRef factory = CFUUIDCreateFromString(kCFAllocatorDefault, CFSTR(FOVEONIC_SPOTLIGHT_FACTORY));
    if (!factory) return NULL;
    Importer *importer = create(factory);
    CFRelease(factory);
    return importer;
}
