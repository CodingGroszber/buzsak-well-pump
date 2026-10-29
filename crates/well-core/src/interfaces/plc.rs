//! The physical interfaces of the Industrial Shields ESP32 PLC 14 IO (board `14iosplc_v`,
//! hardware v4, 0-10 V analog inputs).
//!
//! Every struct and constant is named after the terminal or pin on the board and carries the
//! values of the Industrial Shields Arduino core (`variants/14iosplc/pins_is.h`). This module
//! describes the hardware only; what a terminal is *used for* belongs to the control layer.
//!
//! Signals live in two places:
//! - directly on an ESP32 pin ([`GpioPin`]), or
//! - on the on-board MCP23017 I2C expander ([`ExpanderPin`]). The Arduino core addresses these
//!   as `0x2000 + index`; only the index is stored here.

/// A signal wired directly to an ESP32 GPIO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpioPin(
    /// The ESP32 GPIO number.
    pub u8,
);

/// A signal on the MCP23017 expander: index 0-7 is port A (`GPA0-7`), 8-15 is port B (`GPB0-7`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpanderPin(
    /// The expander pin index, 0-15 (the Arduino core's `0x2000 + index` code).
    pub u8,
);

/// A digital input terminal (`I0_0` ... `I0_6`), 5-24 V, opto-isolated, read through the expander.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DigitalInput {
    /// Terminal name on the board, e.g. `"I0_0"`.
    pub terminal: &'static str,
    /// The expander pin that carries the input.
    pub pin: ExpanderPin,
}

/// An analog input terminal (`I0_7`, `I0_8`), read by an ESP32 ADC1 channel.
///
/// In the 0-10 V variant the input is routed to a different GPIO than in the 4-20 mA variant;
/// the enable pin selects the front end and is driven LOW at boot for the 0-10 V variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalogInput {
    /// Terminal name on the board, e.g. `"I0_7"`.
    pub terminal: &'static str,
    /// Alias in the Arduino core, e.g. `"AI0_0"`.
    pub alias: &'static str,
    /// The GPIO (ADC1 channel) that carries the 0-10 V signal.
    pub gpio_v: GpioPin,
    /// The GPIO that carries the signal in the 4-20 mA variant of the board.
    pub gpio_ma: GpioPin,
    /// The GPIO that enables the 4-20 mA front end (`AC7_EN` / `AC8_EN`); LOW selects 0-10 V.
    pub front_end_enable: GpioPin,
}

/// A transistor output terminal (`Q0_0` ... `Q0_3`): 70 mA, switching either 5 V or 24 V.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransistorOutput {
    /// Terminal name on the board, e.g. `"Q0_0"`.
    pub terminal: &'static str,
    /// The GPIO that switches the output.
    pub gpio: GpioPin,
    /// Name of the supply-select signal, e.g. `"S0_24v"`.
    pub supply_select_name: &'static str,
    /// The expander pin that selects the output supply: HIGH = 24 V, LOW = 5 V (set before use).
    pub supply_select: ExpanderPin,
}

/// The relay output terminal (`R0_0`, alias `REL`): 250 V AC 5 A / 30 V DC 3 A, resistive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelayOutput {
    /// Terminal name on the board, e.g. `"R0_0"`.
    pub terminal: &'static str,
    /// Alias in the Arduino core, e.g. `"REL"`.
    pub alias: &'static str,
    /// The expander pin that drives the relay coil.
    pub pin: ExpanderPin,
}

/// The I2C bus that connects the ESP32 to the MCP23017 expander.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpanderBus {
    /// I2C controller number of the ESP32.
    pub controller: u8,
    /// The I2C data pin.
    pub sda: GpioPin,
    /// The I2C clock pin.
    pub scl: GpioPin,
    /// 7-bit I2C address of the MCP23017.
    pub address: u8,
}

/// The RS-485 interface (half duplex, 120 ohm termination on the board).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rs485 {
    /// ESP32 UART number.
    pub uart: u8,
    /// Driver-enable / receiver-enable pin (`RS485_DE_RE`).
    pub de_re: GpioPin,
}

/// The SPI bus and the chip select of the on-board Ethernet controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EthernetSpi {
    /// SPI MOSI pin.
    pub mosi: GpioPin,
    /// SPI MISO pin.
    pub miso: GpioPin,
    /// SPI clock pin.
    pub sck: GpioPin,
    /// Chip select of the Ethernet controller (`PIN_SPI_SS_ETHERNET_LIB`).
    pub cs: GpioPin,
}

/// The expander pins that are not user terminals (expansion connector and board internals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpanderAuxiliary {
    /// `PWM` (`0x2009`).
    pub pwm: ExpanderPin,
    /// `INT` (`0x2008`).
    pub int: ExpanderPin,
    /// `AN` (`0x2002`).
    pub an: ExpanderPin,
    /// `RST` (`0x2003`).
    pub rst: ExpanderPin,
}

/// The digital input terminals `I0_0` ... `I0_6` and their expander pins.
pub const DIGITAL_INPUTS: [DigitalInput; 7] = [
    DigitalInput {
        terminal: "I0_0",
        pin: ExpanderPin(0x0B),
    },
    DigitalInput {
        terminal: "I0_1",
        pin: ExpanderPin(0x0A),
    },
    DigitalInput {
        terminal: "I0_2",
        pin: ExpanderPin(0x0C),
    },
    DigitalInput {
        terminal: "I0_3",
        pin: ExpanderPin(0x0D),
    },
    DigitalInput {
        terminal: "I0_4",
        pin: ExpanderPin(0x04),
    },
    DigitalInput {
        terminal: "I0_5",
        pin: ExpanderPin(0x06),
    },
    DigitalInput {
        terminal: "I0_6",
        pin: ExpanderPin(0x05),
    },
];

/// The analog input terminals `I0_7` and `I0_8` with their GPIOs and front-end enables.
pub const ANALOG_INPUTS: [AnalogInput; 2] = [
    AnalogInput {
        terminal: "I0_7",
        alias: "AI0_0",
        gpio_v: GpioPin(34),
        gpio_ma: GpioPin(32),
        front_end_enable: GpioPin(15),
    },
    AnalogInput {
        terminal: "I0_8",
        alias: "AI0_1",
        gpio_v: GpioPin(35),
        gpio_ma: GpioPin(33),
        front_end_enable: GpioPin(2),
    },
];

/// The transistor output terminals `Q0_0` ... `Q0_3`, their GPIOs and supply-select pins.
pub const TRANSISTOR_OUTPUTS: [TransistorOutput; 4] = [
    TransistorOutput {
        terminal: "Q0_0",
        gpio: GpioPin(12),
        supply_select_name: "S0_24v",
        supply_select: ExpanderPin(0x0F),
    },
    TransistorOutput {
        terminal: "Q0_1",
        gpio: GpioPin(25),
        supply_select_name: "S1_24v",
        supply_select: ExpanderPin(0x0E),
    },
    TransistorOutput {
        terminal: "Q0_2",
        gpio: GpioPin(26),
        supply_select_name: "S2_24v",
        supply_select: ExpanderPin(0x01),
    },
    TransistorOutput {
        terminal: "Q0_3",
        gpio: GpioPin(27),
        supply_select_name: "S3_24v",
        supply_select: ExpanderPin(0x00),
    },
];

/// The relay output terminal `R0_0` (`REL`).
pub const RELAY_OUTPUT: RelayOutput = RelayOutput {
    terminal: "R0_0",
    alias: "REL",
    pin: ExpanderPin(0x07),
};

/// The I2C bus to the expander: controller 0, SDA GPIO 21, SCL GPIO 22, address `0x20`.
pub const EXPANDER_BUS: ExpanderBus = ExpanderBus {
    controller: 0,
    sda: GpioPin(21),
    scl: GpioPin(22),
    address: 0x20,
};

/// The RS-485 interface: UART 2 with driver-enable on GPIO 5.
pub const RS485: Rs485 = Rs485 {
    uart: 2,
    de_re: GpioPin(5),
};

/// The Ethernet SPI bus: MOSI 23, MISO 19, SCK 18, chip select GPIO 14.
pub const ETHERNET_SPI: EthernetSpi = EthernetSpi {
    mosi: GpioPin(23),
    miso: GpioPin(19),
    sck: GpioPin(18),
    cs: GpioPin(14),
};

/// The expander pins of the expansion connector and board internals.
pub const EXPANDER_AUXILIARY: ExpanderAuxiliary = ExpanderAuxiliary {
    pwm: ExpanderPin(0x09),
    int: ExpanderPin(0x08),
    an: ExpanderPin(0x02),
    rst: ExpanderPin(0x03),
};

/// ADC resolution configured by the Arduino core for the analog inputs, in bits.
pub const ADC_RESOLUTION_BITS: u8 = 12;

/// ADC input attenuation configured by the Arduino core, in dB (full scale about 3.3 V).
pub const ADC_ATTENUATION_DB: u8 = 11;

/// Levels of the digital inputs `I0_0` ... `I0_6`; `true` means the input pin reads HIGH.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DigitalInputs {
    /// Level of `I0_0`.
    pub i0_0: bool,
    /// Level of `I0_1`.
    pub i0_1: bool,
    /// Level of `I0_2`.
    pub i0_2: bool,
    /// Level of `I0_3`.
    pub i0_3: bool,
    /// Level of `I0_4`.
    pub i0_4: bool,
    /// Level of `I0_5`.
    pub i0_5: bool,
    /// Level of `I0_6`.
    pub i0_6: bool,
}

/// Voltages at the ADC pins of the analog inputs, in millivolts (0 = no signal).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalogInputs {
    /// Millivolts measured for `I0_7` (`AI0_0`).
    pub i0_7_mv: u32,
    /// Millivolts measured for `I0_8` (`AI0_1`).
    pub i0_8_mv: u32,
}

/// Everything that can be read from the board in one scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlcInputs {
    /// Digital inputs `I0_0` ... `I0_6`.
    pub digital: DigitalInputs,
    /// Analog inputs `I0_7` and `I0_8`.
    pub analog: AnalogInputs,
}

/// Levels of the transistor outputs `Q0_0` ... `Q0_3`; `true` drives the output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransistorOutputs {
    /// Level of `Q0_0`.
    pub q0_0: bool,
    /// Level of `Q0_1`.
    pub q0_1: bool,
    /// Level of `Q0_2`.
    pub q0_2: bool,
    /// Level of `Q0_3`.
    pub q0_3: bool,
}

/// Supply selection of the transistor outputs; `true` selects 24 V, `false` selects 5 V.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputSupplies {
    /// Supply of `Q0_0` (`S0_24v`).
    pub s0_24v: bool,
    /// Supply of `Q0_1` (`S1_24v`).
    pub s1_24v: bool,
    /// Supply of `Q0_2` (`S2_24v`).
    pub s2_24v: bool,
    /// Supply of `Q0_3` (`S3_24v`).
    pub s3_24v: bool,
}

/// Everything that can be written to the board in one scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlcOutputs {
    /// Transistor outputs `Q0_0` ... `Q0_3`.
    pub transistors: TransistorOutputs,
    /// Supply selection of those outputs.
    pub supplies: OutputSupplies,
    /// Level of the relay output `R0_0`.
    pub r0_0: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expander_pins_match_the_arduino_core_codes() {
        // 0x200B, 0x200A, 0x200C, 0x200D, 0x2004, 0x2006, 0x2005 in pins_is.h.
        let indices: Vec<u8> = DIGITAL_INPUTS.iter().map(|i| i.pin.0).collect();
        assert_eq!(indices, [0x0B, 0x0A, 0x0C, 0x0D, 0x04, 0x06, 0x05]);
        assert_eq!(RELAY_OUTPUT.pin, ExpanderPin(7));
    }

    #[test]
    fn no_expander_pin_is_used_twice() {
        let mut pins: Vec<u8> = DIGITAL_INPUTS.iter().map(|i| i.pin.0).collect();
        pins.extend(TRANSISTOR_OUTPUTS.iter().map(|o| o.supply_select.0));
        pins.push(RELAY_OUTPUT.pin.0);
        let aux = EXPANDER_AUXILIARY;
        pins.extend([aux.pwm.0, aux.int.0, aux.an.0, aux.rst.0]);
        let mut unique = pins.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), pins.len(), "expander pins collide: {pins:?}");
        assert!(pins.iter().all(|&p| p < 16));
    }

    #[test]
    fn no_gpio_is_used_twice() {
        let mut gpios: Vec<u8> = ANALOG_INPUTS
            .iter()
            .flat_map(|a| [a.gpio_v.0, a.gpio_ma.0, a.front_end_enable.0])
            .collect();
        gpios.extend(TRANSISTOR_OUTPUTS.iter().map(|o| o.gpio.0));
        gpios.extend([EXPANDER_BUS.sda.0, EXPANDER_BUS.scl.0, RS485.de_re.0]);
        gpios.extend([
            ETHERNET_SPI.mosi.0,
            ETHERNET_SPI.miso.0,
            ETHERNET_SPI.sck.0,
            ETHERNET_SPI.cs.0,
        ]);
        let mut unique = gpios.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), gpios.len(), "GPIOs collide: {gpios:?}");
    }

    #[test]
    fn terminals_are_named_in_order() {
        let outputs: Vec<&str> = TRANSISTOR_OUTPUTS.iter().map(|o| o.terminal).collect();
        assert_eq!(outputs, ["Q0_0", "Q0_1", "Q0_2", "Q0_3"]);
        let inputs: Vec<&str> = DIGITAL_INPUTS.iter().map(|i| i.terminal).collect();
        assert_eq!(
            inputs,
            ["I0_0", "I0_1", "I0_2", "I0_3", "I0_4", "I0_5", "I0_6"]
        );
    }
}
