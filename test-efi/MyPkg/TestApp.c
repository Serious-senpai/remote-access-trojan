#include <Uefi.h>
#include <Library/UefiLib.h>
#include <Library/UefiBootServicesTableLib.h>

EFI_STATUS
EFIAPI
UefiMain(
    IN EFI_HANDLE ImageHandle,
    IN EFI_SYSTEM_TABLE *SystemTable)
{
    Print(L"[+] Hello from TestApp!\n");

    for (UINT32 i = 0; i < 5; i++)
    {
        Print(L"[+] Counting down %d seconds\n", 5 - i);
        gBS->Stall(1000000);
    }

    Print(L"[+] Loading bootmgfw_old.efi...\n");

    EFI_HANDLE NewImageHandle = NULL;
    EFI_STATUS Status = gBS->LoadImage(
        FALSE,
        ImageHandle,
        NULL,
        L"\\EFI\\Microsoft\\Boot\\bootmgfw_old.efi",
        0,
        &NewImageHandle);

    if (EFI_ERROR(Status))
    {
        Print(L"[-] LoadImage failed: %r\n", Status);
        gBS->Stall(5000000);
        return Status;
    }

    for (UINT32 i = 0; i < 5; i++)
    {
        Print(L"[+] Counting down %d seconds\n", 5 - i);
        gBS->Stall(1000000);
    }

    Print(L"[+] Starting image...\n");

    Status = gBS->StartImage(NewImageHandle, NULL, NULL);

    Print(L"[+] Returned from bootmgfw_old.efi: %r\n", Status);

    return EFI_SUCCESS;
}
