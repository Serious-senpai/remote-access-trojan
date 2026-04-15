use alloc::string::String;

use uefi::data_types::FromStrWithBufError;
use uefi::proto::device_path::DevicePathUtilitiesError;
use uefi::proto::device_path::build::BuildError;

pub trait UefiErrorMessage {
    fn convert(self, message: impl Into<String>) -> uefi::Error<String>;
}

impl UefiErrorMessage for uefi::Error<()> {
    fn convert(self, message: impl Into<String>) -> uefi::Error<String> {
        uefi::Error::new(self.status(), message.into())
    }
}

impl UefiErrorMessage for FromStrWithBufError {
    fn convert(self, message: impl Into<String>) -> uefi::Error<String> {
        uefi::Error::new(
            match self {
                Self::BufferTooSmall => uefi::Status::BUFFER_TOO_SMALL,
                Self::InvalidChar(..) | Self::InteriorNul(..) => uefi::Status::COMPROMISED_DATA,
            },
            message.into(),
        )
    }
}

impl UefiErrorMessage for BuildError {
    fn convert(self, message: impl Into<String>) -> uefi::Error<String> {
        uefi::Error::new(
            match self {
                Self::BufferTooSmall => uefi::Status::BUFFER_TOO_SMALL,
                Self::NodeTooBig | Self::UnexpectedEndEntire => uefi::Status::COMPROMISED_DATA,
            },
            message.into(),
        )
    }
}

impl UefiErrorMessage for DevicePathUtilitiesError {
    fn convert(self, message: impl Into<String>) -> uefi::Error<String> {
        uefi::Error::new(
            match self {
                Self::CantLocateHandleBuffer(..) | Self::CantOpenProtocol(..) | Self::NoHandle => {
                    uefi::Status::NOT_FOUND
                }
                Self::OutOfMemory => uefi::Status::OUT_OF_RESOURCES,
            },
            message.into(),
        )
    }
}

pub trait UefiResultConvertable {
    type Output;

    fn convert(self, message: impl Into<String>) -> uefi::Result<Self::Output, String>;
}

impl<T, E: UefiErrorMessage> UefiResultConvertable for Result<T, E> {
    type Output = T;

    fn convert(self, message: impl Into<String>) -> uefi::Result<Self::Output, String> {
        self.map_err(|e| e.convert(message))
    }
}
