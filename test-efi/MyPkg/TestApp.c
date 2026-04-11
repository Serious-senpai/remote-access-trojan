#include <Uefi.h>
#include <Library/UefiLib.h>
#include <Library/UefiBootServicesTableLib.h>
#include <Library/DevicePathLib.h>
#include <Protocol/LoadedImage.h>

#define LOG0(fmt) Print(L"[%a:%u] " fmt, __FILE__, __LINE__)

/** Format specifier list: https://github.com/tianocore/edk2/blob/master/MdePkg/Include/Library/PrintLib.h */
#define LOG(fmt, ...) Print(L"[%a:%u] " fmt, __FILE__, __LINE__, __VA_ARGS__)

VOID Countdown(IN UINTN Seconds)
{
    for (UINTN i = 0; i < Seconds; i++)
    {
        LOG("Counting down %d seconds\n", Seconds - i);
        gBS->Stall(1000000);
    }
}

#define ERROR_CHECK(status, fmt)  \
    if (EFI_ERROR(status))        \
    {                             \
        LOG(fmt, status, status); \
        Countdown(5);             \
        return status;            \
    }

EFI_STATUS EFIAPI UefiMain(IN EFI_HANDLE ImageHandle, IN EFI_SYSTEM_TABLE *SystemTable)
{
    LOG0("Loading bootmgfw_old.efi...\n");

    EFI_LOADED_IMAGE_PROTOCOL *LoadedImage = NULL;
    EFI_STATUS Status = gBS->HandleProtocol(ImageHandle, &gEfiLoadedImageProtocolGuid, (VOID **)&LoadedImage);
    ERROR_CHECK(Status, "HandleProtocol failed: %r (0x%x)\n");

    EFI_HANDLE WindowsImageHandle = NULL;
    {
        // Avoid leaking DevicePath outside of this scope
        EFI_DEVICE_PATH_PROTOCOL *DevicePath = FileDevicePath(LoadedImage->DeviceHandle, L"\\EFI\\Microsoft\\Boot\\bootmgfw_old.efi");
        if (DevicePath == NULL)
        {
            LOG0("FileDevicePath failed\n");
            Countdown(5);
            return EFI_OUT_OF_RESOURCES;
        }

        Status = gBS->LoadImage(
            FALSE,
            ImageHandle,
            DevicePath,
            NULL,
            0,
            &WindowsImageHandle);
        gBS->FreePool(DevicePath); // We will not use it anymore, so free the allocated memory
        ERROR_CHECK(Status, "LoadImage failed: %r (0x%x)\n");
    }

    LOG0("Loaded bootmgfw_old.efi\n");

    EFI_LOADED_IMAGE_PROTOCOL *WindowsLoadedImage = NULL;
    Status = gBS->HandleProtocol(WindowsImageHandle, &gEfiLoadedImageProtocolGuid, (VOID **)&WindowsLoadedImage);
    ERROR_CHECK(Status, "HandleProtocol failed: %r (0x%x)\n");

    // Read first 10 bytes
    UINT8 *ImageBase = (UINT8 *)WindowsLoadedImage->ImageBase;
    for (UINT8 *p = ImageBase; p < ImageBase + 10; p++)
    {
        LOG("Byte at 0x%p: 0x%02x (ASCII %c)\n", p, *p, *p);
    }

    LOG0("Starting image...\n");

    UINTN ExitDataSize = 0;
    Status = gBS->StartImage(WindowsImageHandle, &ExitDataSize, NULL);

    // Control should not reach here
    LOG("Returned from bootmgfw_old.efi: %r (0x%x)\n", Status, Status);
    return Status;
}
