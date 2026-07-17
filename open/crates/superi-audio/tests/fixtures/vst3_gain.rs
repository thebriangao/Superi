#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::{c_char, c_void, CStr};
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicI64, AtomicPtr, AtomicU32, AtomicU64, Ordering,
};

use vst3::{uid, Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

const EFFECT_NAME: &str = "Superi VST3 gain fixture";
const PARAMETER_ID: u32 = 0;
const READ_ONLY_PARAMETER_ID: u32 = 1;
const MAXIMUM_POINTS: usize = 64;

static EVENT_COUNT: AtomicU32 = AtomicU32::new(0);
static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static OBSERVED_SAMPLE_RATE: AtomicU64 = AtomicU64::new(0);
static OBSERVED_START_SAMPLE: AtomicI64 = AtomicI64::new(0);
static OBSERVED_FRAMES: AtomicI32 = AtomicI32::new(0);
static OBSERVED_MODE: AtomicI32 = AtomicI32::new(-1);
static OBSERVED_SAMPLE_SIZE: AtomicI32 = AtomicI32::new(-1);
static OBSERVED_CHANNELS: AtomicI32 = AtomicI32::new(0);
static PROCESS_COUNT: AtomicU32 = AtomicU32::new(0);
static HOST_OBJECTS_VERIFIED: AtomicBool = AtomicBool::new(false);
static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static CALLBACK_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static COMPONENT_STATE_GETS: AtomicU32 = AtomicU32::new(0);
static COMPONENT_STATE_SETS: AtomicU32 = AtomicU32::new(0);
static CONTROLLER_COMPONENT_STATE_SETS: AtomicU32 = AtomicU32::new(0);
static CONTROLLER_STATE_GETS: AtomicU32 = AtomicU32::new(0);
static CONTROLLER_STATE_SETS: AtomicU32 = AtomicU32::new(0);

struct CountingAllocator;

// SAFETY: System remains the sole backing allocator. The atomic counters observe calls without
// changing pointer, layout, ownership, or lifetime behavior.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            CALLBACK_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: The exact caller-provided layout is forwarded to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            CALLBACK_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: The pointer and layout are forwarded unchanged to their backing allocator.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            CALLBACK_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: The exact caller-provided layout is forwarded to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            CALLBACK_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: The original pointer and layout plus requested size are forwarded unchanged.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct AllocationWindow;

impl AllocationWindow {
    fn enter() -> Self {
        TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for AllocationWindow {
    fn drop(&mut self) {
        TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
    }
}

fn record_event(code: u64) {
    let index = EVENT_COUNT.fetch_add(1, Ordering::SeqCst);
    if index < 16 {
        EVENT_SEQUENCE.fetch_or(code << (index * 4), Ordering::SeqCst);
    }
}

fn copy_c_string(source: &str, destination: &mut [c_char]) {
    destination.fill(0);
    let capacity = destination.len().saturating_sub(1);
    for (source, destination) in source.bytes().zip(destination.iter_mut().take(capacity)) {
        *destination = source as c_char;
    }
}

fn copy_utf16(source: &str, destination: &mut [TChar]) {
    destination.fill(0);
    let capacity = destination.len().saturating_sub(1);
    for (source, destination) in source
        .encode_utf16()
        .zip(destination.iter_mut().take(capacity))
    {
        *destination = source as TChar;
    }
}

fn configured_arrangement() -> SpeakerArrangement {
    std::env::var("SUPERI_VST3_FIXTURE_ARRANGEMENT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(SpeakerArr::kStereo)
}

fn arrangement_channels(arrangement: SpeakerArrangement) -> Option<i32> {
    match arrangement {
        SpeakerArr::kMono => Some(1),
        SpeakerArr::kStereo => Some(2),
        SpeakerArr::k40Music => Some(4),
        SpeakerArr::k51 => Some(6),
        SpeakerArr::k71Music => Some(8),
        _ => None,
    }
}

unsafe fn write_gain_state(stream: *mut IBStream, gain: u64) -> tresult {
    // SAFETY: The host supplies one live stream for this synchronous state call.
    let Some(stream) = (unsafe { ComRef::from_raw(stream) }) else {
        return kInvalidArgument;
    };
    let bytes = gain.to_le_bytes();
    let mut written = 0_i32;
    // SAFETY: bytes remains readable and written remains writable for the complete call.
    let result = unsafe {
        stream.write(
            bytes.as_ptr().cast_mut().cast::<c_void>(),
            i32::try_from(bytes.len()).unwrap(),
            &mut written,
        )
    };
    if result == kResultOk && written == i32::try_from(bytes.len()).unwrap() {
        kResultOk
    } else {
        kResultFalse
    }
}

unsafe fn read_gain_state(stream: *mut IBStream) -> Result<u64, tresult> {
    // SAFETY: The host supplies one live stream for this synchronous state call.
    let Some(stream) = (unsafe { ComRef::from_raw(stream) }) else {
        return Err(kInvalidArgument);
    };
    let mut bytes = [0_u8; 8];
    let mut read = 0_i32;
    // SAFETY: bytes remains writable and read remains writable for the complete call.
    let result = unsafe {
        stream.read(
            bytes.as_mut_ptr().cast::<c_void>(),
            i32::try_from(bytes.len()).unwrap(),
            &mut read,
        )
    };
    if result == kResultOk && read == i32::try_from(bytes.len()).unwrap() {
        Ok(u64::from_le_bytes(bytes))
    } else {
        Err(kResultFalse)
    }
}

struct FixtureEffect {
    gain: AtomicU64,
    arrangement: AtomicU64,
    host_context: AtomicPtr<FUnknown>,
    host_message: AtomicPtr<IMessage>,
    host_attributes: AtomicPtr<IAttributeList>,
}

impl FixtureEffect {
    const CID: TUID = uid(0x6E332252, 0x54224A00, 0xAA69301A, 0xF318797D);

    fn new() -> Self {
        Self {
            gain: AtomicU64::new(1.0_f64.to_bits()),
            arrangement: AtomicU64::new(configured_arrangement()),
            host_context: AtomicPtr::new(ptr::null_mut()),
            host_message: AtomicPtr::new(ptr::null_mut()),
            host_attributes: AtomicPtr::new(ptr::null_mut()),
        }
    }
}

impl Class for FixtureEffect {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl IPluginBaseTrait for FixtureEffect {
    unsafe fn initialize(&self, context: *mut FUnknown) -> tresult {
        // SAFETY: The host context remains alive for the component's initialized lifetime.
        let Some(context_reference) = (unsafe { ComRef::from_raw(context) }) else {
            return kInvalidArgument;
        };
        let Some(host) = context_reference.cast::<IHostApplication>() else {
            return kNoInterface;
        };
        let mut host_name = [0 as TChar; 128];
        // SAFETY: host_name is exact writable String128 storage.
        if unsafe { host.getName(&mut host_name) } != kResultOk
            || host_name[0] != b'S' as TChar
        {
            return kResultFalse;
        }
        let Some(support) = context_reference.cast::<IPlugInterfaceSupport>() else {
            return kNoInterface;
        };
        let handler_id = IComponentHandler_iid;
        let handler2_id = IComponentHandler2_iid;
        // SAFETY: Both identifiers are exact readable TUID values for synchronous queries.
        if unsafe { support.isPlugInterfaceSupported(&handler_id) } != kResultTrue
            || unsafe { support.isPlugInterfaceSupported(&handler2_id) } != kResultFalse
        {
            return kResultFalse;
        }

        let mut message_class = IMessage_iid;
        let mut message_interface = IMessage_iid;
        let mut message_object = ptr::null_mut();
        // SAFETY: All identifiers and the output pointer remain live for this call.
        if unsafe {
            host.createInstance(
                &mut message_class,
                &mut message_interface,
                &mut message_object,
            )
        } != kResultOk
        {
            return kNoInterface;
        }
        // SAFETY: A successful createInstance transferred one owned IMessage reference.
        let Some(message) = (unsafe { ComPtr::<IMessage>::from_raw(message_object.cast()) }) else {
            return kNoInterface;
        };
        let message_id = b"fixture-message\0";
        // SAFETY: message_id is one readable null-terminated identifier.
        unsafe { message.setMessageID(message_id.as_ptr().cast::<c_char>()) };
        // SAFETY: The returned identifier remains owned by the message.
        let observed_message_id = unsafe { message.getMessageID() };
        if observed_message_id.is_null()
            // SAFETY: A nonnull returned message ID is null terminated and retained by the host.
            || unsafe { CStr::from_ptr(observed_message_id) }.to_bytes() != b"fixture-message"
        {
            return kResultFalse;
        }
        // SAFETY: The returned attribute-list pointer is borrowed from the retained message.
        let attributes_pointer = unsafe { message.getAttributes() };
        // SAFETY: A nonnull returned pointer is a live IAttributeList owned by message.
        let Some(attributes) = (unsafe { ComRef::from_raw(attributes_pointer) }) else {
            return kNoInterface;
        };
        let integer_key = b"integer\0";
        let float_key = b"float\0";
        let string_key = b"string\0";
        let binary_key = b"binary\0";
        let mut integer = 0_i64;
        let mut float = 0_f64;
        let string = [b'h' as TChar, b'o' as TChar, b's' as TChar, b't' as TChar, 0];
        let mut observed_string = [0 as TChar; 16];
        let binary = [1_u8, 3, 5, 7];
        let mut observed_binary = ptr::null();
        let mut observed_binary_size = 0_u32;
        // SAFETY: Keys and values are valid synchronous IAttributeList arguments.
        if unsafe { attributes.setInt(integer_key.as_ptr().cast::<c_char>(), 41) } != kResultOk
            || unsafe { attributes.getInt(integer_key.as_ptr().cast::<c_char>(), &mut integer) }
                != kResultOk
            || integer != 41
            || unsafe { attributes.setFloat(float_key.as_ptr().cast::<c_char>(), 0.625) }
                != kResultOk
            || unsafe { attributes.getFloat(float_key.as_ptr().cast::<c_char>(), &mut float) }
                != kResultOk
            || float != 0.625
            || unsafe {
                attributes.setString(string_key.as_ptr().cast::<c_char>(), string.as_ptr())
            } != kResultOk
            || unsafe {
                attributes.getString(
                    string_key.as_ptr().cast::<c_char>(),
                    observed_string.as_mut_ptr(),
                    u32::try_from(mem::size_of_val(&observed_string)).unwrap(),
                )
            } != kResultOk
            || observed_string[..string.len()] != string
            || unsafe {
                attributes.setBinary(
                    binary_key.as_ptr().cast::<c_char>(),
                    binary.as_ptr().cast::<c_void>(),
                    u32::try_from(binary.len()).unwrap(),
                )
            } != kResultOk
            || unsafe {
                attributes.getBinary(
                    binary_key.as_ptr().cast::<c_char>(),
                    &mut observed_binary,
                    &mut observed_binary_size,
                )
            } != kResultOk
            || observed_binary_size != u32::try_from(binary.len()).unwrap()
            || observed_binary.is_null()
            // SAFETY: getBinary returned observed_binary_size readable retained bytes.
            || unsafe {
                slice::from_raw_parts(observed_binary.cast::<u8>(), observed_binary_size as usize)
            } != binary
        {
            return kResultFalse;
        }

        let mut attributes_class = IAttributeList_iid;
        let mut attributes_interface = IAttributeList_iid;
        let mut direct_object = ptr::null_mut();
        // SAFETY: All identifiers and the output pointer remain live for this call.
        if unsafe {
            host.createInstance(
                &mut attributes_class,
                &mut attributes_interface,
                &mut direct_object,
            )
        } != kResultOk
        {
            return kNoInterface;
        }
        // SAFETY: A successful createInstance transferred one owned IAttributeList reference.
        let Some(direct_attributes) =
            (unsafe { ComPtr::<IAttributeList>::from_raw(direct_object.cast()) })
        else {
            return kNoInterface;
        };
        let direct_key = b"direct\0";
        let mut direct_value = 0_i64;
        // SAFETY: The key and output value are valid synchronous attribute arguments.
        if unsafe { direct_attributes.setInt(direct_key.as_ptr().cast::<c_char>(), 73) }
            != kResultOk
            || unsafe {
                direct_attributes.getInt(direct_key.as_ptr().cast::<c_char>(), &mut direct_value)
            } != kResultOk
            || direct_value != 73
        {
            return kResultFalse;
        }

        self.host_context.store(context, Ordering::Release);
        self.host_attributes
            .store(attributes_pointer, Ordering::Release);
        self.host_message
            .store(message.into_raw(), Ordering::Release);
        HOST_OBJECTS_VERIFIED.store(true, Ordering::Release);
        record_event(2);
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        self.host_context.store(ptr::null_mut(), Ordering::Release);
        self.host_attributes
            .store(ptr::null_mut(), Ordering::Release);
        let message = self.host_message.swap(ptr::null_mut(), Ordering::AcqRel);
        if !message.is_null() {
            // SAFETY: initialize stored exactly one owned IMessage reference for reverse teardown.
            drop(unsafe { ComPtr::<IMessage>::from_raw_unchecked(message) });
        }
        record_event(11);
        kResultOk
    }
}

impl IComponentTrait for FixtureEffect {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, direction: BusDirection) -> i32 {
        if media_type == MediaTypes_::kAudio as MediaType
            && matches!(
                direction,
                value if value == BusDirections_::kInput as BusDirection
                    || value == BusDirections_::kOutput as BusDirection
            )
        {
            1
        } else {
            0
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        direction: BusDirection,
        index: i32,
        info: *mut BusInfo,
    ) -> tresult {
        if media_type != MediaTypes_::kAudio as MediaType
            || index != 0
            || info.is_null()
            || (direction != BusDirections_::kInput as BusDirection
                && direction != BusDirections_::kOutput as BusDirection)
        {
            return kInvalidArgument;
        }
        let Some(channels) = arrangement_channels(self.arrangement.load(Ordering::Relaxed)) else {
            return kResultFalse;
        };
        // SAFETY: The host supplied one writable BusInfo after validating the matching bus index.
        let info = unsafe { &mut *info };
        info.mediaType = media_type;
        info.direction = direction;
        info.channelCount = channels;
        copy_utf16(
            if direction == BusDirections_::kInput as BusDirection {
                "Input"
            } else {
                "Output"
            },
            &mut info.name,
        );
        info.busType = BusTypes_::kMain as BusType;
        info.flags = BusInfo_::BusFlags_::kDefaultActive;
        kResultOk
    }

    unsafe fn getRoutingInfo(
        &self,
        _input: *mut RoutingInfo,
        _output: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        media_type: MediaType,
        direction: BusDirection,
        index: i32,
        state: TBool,
    ) -> tresult {
        if media_type != MediaTypes_::kAudio as MediaType
            || index != 0
            || (direction != BusDirections_::kInput as BusDirection
                && direction != BusDirections_::kOutput as BusDirection)
        {
            return kInvalidArgument;
        }
        if state != 0
            && direction == BusDirections_::kOutput as BusDirection
            && std::env::var_os("SUPERI_VST3_FIXTURE_FAIL_OUTPUT_ACTIVATION").is_some()
        {
            record_event(13);
            return kResultFalse;
        }
        record_event(if state != 0 { 4 } else { 10 });
        kResultOk
    }

    unsafe fn setActive(&self, state: TBool) -> tresult {
        record_event(if state != 0 { 5 } else { 9 });
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        // SAFETY: The exact state stream is consumed synchronously during host preparation.
        match unsafe { read_gain_state(state) } {
            Ok(gain) => {
                self.gain.store(gain, Ordering::Relaxed);
                COMPONENT_STATE_SETS.fetch_add(1, Ordering::Relaxed);
                kResultOk
            }
            Err(result) => result,
        }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        COMPONENT_STATE_GETS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The host-provided stream remains live for this synchronous state write.
        unsafe { write_gain_state(state, self.gain.load(Ordering::Relaxed)) }
    }
}

impl IAudioProcessorTrait for FixtureEffect {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        input_count: i32,
        outputs: *mut SpeakerArrangement,
        output_count: i32,
    ) -> tresult {
        if input_count != 1 || output_count != 1 || inputs.is_null() || outputs.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied exactly one readable arrangement for each nonnull pointer.
        let input = unsafe { *inputs };
        // SAFETY: The output pointer has the same validated one-element extent.
        let output = unsafe { *outputs };
        if input != output || arrangement_channels(input).is_none() {
            return kResultFalse;
        }
        self.arrangement.store(input, Ordering::Relaxed);
        kResultTrue
    }

    unsafe fn getBusArrangement(
        &self,
        direction: BusDirection,
        index: i32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        if index != 0
            || arrangement.is_null()
            || (direction != BusDirections_::kInput as BusDirection
                && direction != BusDirections_::kOutput as BusDirection)
        {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied one writable arrangement for the validated bus.
        unsafe { arrangement.write(self.arrangement.load(Ordering::Relaxed)) };
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, sample_size: i32) -> tresult {
        if sample_size == SymbolicSampleSizes_::kSample32 as i32 {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        7
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        if setup.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied one readable ProcessSetup for this synchronous call.
        let setup = unsafe { &*setup };
        OBSERVED_SAMPLE_RATE.store(setup.sampleRate.to_bits(), Ordering::Relaxed);
        OBSERVED_MODE.store(setup.processMode, Ordering::Relaxed);
        OBSERVED_SAMPLE_SIZE.store(setup.symbolicSampleSize, Ordering::Relaxed);
        record_event(3);
        kResultOk
    }

    unsafe fn setProcessing(&self, state: TBool) -> tresult {
        if state == 0 && std::env::var_os("SUPERI_VST3_FIXTURE_FAIL_STOP").is_some() {
            record_event(14);
            return kResultFalse;
        }
        record_event(if state != 0 { 6 } else { 8 });
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        let _allocation_window = AllocationWindow::enter();
        if data.is_null() {
            return kInvalidArgument;
        }
        let context_pointer = self.host_context.load(Ordering::Acquire);
        // SAFETY: initialize retained the host context for the component lifetime.
        let Some(context) = (unsafe { ComRef::from_raw(context_pointer) }) else {
            return kNoInterface;
        };
        let Some(host) = context.cast::<IHostApplication>() else {
            return kNoInterface;
        };
        let mut message_class = IMessage_iid;
        let mut message_interface = IMessage_iid;
        let mut message_object = ptr::null_mut();
        // SAFETY: Host identifiers and output storage remain live during the callback query.
        if unsafe {
            host.createInstance(
                &mut message_class,
                &mut message_interface,
                &mut message_object,
            )
        } != kResultFalse
            || !message_object.is_null()
        {
            return kResultFalse;
        }
        let message_pointer = self.host_message.load(Ordering::Acquire);
        // SAFETY: initialize retained one host message until terminate runs after processing stops.
        let Some(message) = (unsafe { ComRef::from_raw(message_pointer) }) else {
            return kNoInterface;
        };
        // SAFETY: Audio-domain host object access must fail before allocation or locking.
        if !unsafe { message.getMessageID() }.is_null()
            || !unsafe { message.getAttributes() }.is_null()
        {
            return kResultFalse;
        }
        let attributes_pointer = self.host_attributes.load(Ordering::Acquire);
        // SAFETY: The retained message owns this attribute list through terminate.
        let Some(attributes) = (unsafe { ComRef::from_raw(attributes_pointer) }) else {
            return kNoInterface;
        };
        let realtime_key = b"realtime\0";
        // SAFETY: The host must reject this valid key before allocation or mutex acquisition.
        if unsafe { attributes.setInt(realtime_key.as_ptr().cast::<c_char>(), 1) } != kResultFalse {
            return kResultFalse;
        }
        // SAFETY: The host supplies one live ProcessData for the duration of this callback.
        let data = unsafe { &mut *data };
        if data.numInputs != 1
            || data.numOutputs != 1
            || data.numSamples < 0
            || data.inputs.is_null()
            || data.outputs.is_null()
            || data.processContext.is_null()
        {
            return kInvalidArgument;
        }
        // SAFETY: ProcessData declares exactly one readable input bus.
        let input_bus = unsafe { &*data.inputs };
        // SAFETY: ProcessData declares exactly one writable output bus.
        let output_bus = unsafe { &mut *data.outputs };
        let channels = input_bus.numChannels;
        if channels <= 0
            || output_bus.numChannels != channels
            || input_bus.__field0.channelBuffers32.is_null()
            || output_bus.__field0.channelBuffers32.is_null()
        {
            return kInvalidArgument;
        }
        let frame_count = data.numSamples as usize;
        // SAFETY: The bus declares channels readable input pointers for the callback duration.
        let input_channels = unsafe {
            slice::from_raw_parts(input_bus.__field0.channelBuffers32, channels as usize)
        };
        // SAFETY: The output bus declares the same number of writable channel pointers.
        let output_channels = unsafe {
            slice::from_raw_parts(output_bus.__field0.channelBuffers32, channels as usize)
        };
        // SAFETY: The nonnull process-context pointer is live for this callback.
        let context = unsafe { &*data.processContext };
        OBSERVED_SAMPLE_RATE.store(context.sampleRate.to_bits(), Ordering::Relaxed);
        OBSERVED_START_SAMPLE.store(context.projectTimeSamples, Ordering::Relaxed);
        OBSERVED_FRAMES.store(data.numSamples, Ordering::Relaxed);
        OBSERVED_MODE.store(data.processMode, Ordering::Relaxed);
        OBSERVED_SAMPLE_SIZE.store(data.symbolicSampleSize, Ordering::Relaxed);
        OBSERVED_CHANNELS.store(channels, Ordering::Relaxed);

        let mut offsets = [0_i32; MAXIMUM_POINTS];
        let mut values = [0_f64; MAXIMUM_POINTS];
        let mut point_count = 0_usize;
        // SAFETY: The optional input change pointer remains live for this callback.
        if let Some(changes) = unsafe { ComRef::from_raw(data.inputParameterChanges) } {
            let queue_count = unsafe { changes.getParameterCount() };
            for queue_index in 0..queue_count {
                // SAFETY: Each queue index is within the count returned by the same object.
                let queue = unsafe { ComRef::from_raw(changes.getParameterData(queue_index)) };
                let Some(queue) = queue else {
                    return kInvalidArgument;
                };
                if unsafe { queue.getParameterId() } != PARAMETER_ID {
                    return kInvalidArgument;
                }
                let count = unsafe { queue.getPointCount() };
                if count < 0 || point_count + count as usize > MAXIMUM_POINTS {
                    return kOutOfMemory;
                }
                for point_index in 0..count {
                    let mut offset = 0;
                    let mut value = 0.0;
                    if unsafe { queue.getPoint(point_index, &mut offset, &mut value) }
                        != kResultTrue
                    {
                        return kInvalidArgument;
                    }
                    offsets[point_count] = offset;
                    values[point_count] = value;
                    point_count += 1;
                }
            }
        }

        let mut gain = f64::from_bits(self.gain.load(Ordering::Relaxed)) as f32;
        let mut point_index = 0_usize;
        for frame in 0..frame_count {
            while point_index < point_count && offsets[point_index] == frame as i32 {
                gain = values[point_index] as f32;
                point_index += 1;
            }
            for channel in 0..channels as usize {
                if input_channels[channel].is_null() || output_channels[channel].is_null() {
                    return kInvalidArgument;
                }
                // SAFETY: Each channel pointer covers numSamples elements and frame is in range.
                let input = unsafe { *input_channels[channel].add(frame) };
                // SAFETY: The matching output pointer is writable for the same frame extent.
                unsafe { *output_channels[channel].add(frame) = input * gain };
            }
        }
        self.gain.store(f64::from(gain).to_bits(), Ordering::Relaxed);
        output_bus.silenceFlags = 0;

        // SAFETY: The optional output change pointer remains live for this callback.
        if point_count != 0 {
            if let Some(changes) = unsafe { ComRef::from_raw(data.outputParameterChanges) } {
                let mut queue_index = 0;
                // SAFETY: The host copies PARAMETER_ID during this synchronous call and writes one
                // optional queue index.
                let queue_pointer = unsafe {
                    changes.addParameterData(&PARAMETER_ID, &mut queue_index)
                };
                // SAFETY: A nonnull returned queue remains owned by the host for this callback.
                let Some(queue) = (unsafe { ComRef::from_raw(queue_pointer) }) else {
                    return kOutOfMemory;
                };
                for index in 0..point_count {
                    let mut inserted = 0;
                    if unsafe { queue.addPoint(offsets[index], values[index], &mut inserted) }
                        != kResultTrue
                    {
                        return kOutOfMemory;
                    }
                }
            }
        }

        PROCESS_COUNT.fetch_add(1, Ordering::Relaxed);
        record_event(7);
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        11
    }
}

impl IProcessContextRequirementsTrait for FixtureEffect {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        if std::env::var_os("SUPERI_VST3_FIXTURE_REQUIRE_TEMPO").is_some() {
            IProcessContextRequirements_::Flags_::kNeedTempo
        } else {
            0
        }
    }
}

impl IEditControllerTrait for FixtureEffect {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        // SAFETY: The host supplies an independent component-state stream at position zero.
        match unsafe { read_gain_state(state) } {
            Ok(gain) => {
                self.gain.store(gain, Ordering::Relaxed);
                CONTROLLER_COMPONENT_STATE_SETS.fetch_add(1, Ordering::Relaxed);
                kResultOk
            }
            Err(result) => result,
        }
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        // SAFETY: The exact controller-state stream is consumed synchronously.
        match unsafe { read_gain_state(state) } {
            Ok(gain) => {
                self.gain.store(gain, Ordering::Relaxed);
                CONTROLLER_STATE_SETS.fetch_add(1, Ordering::Relaxed);
                kResultOk
            }
            Err(result) => result,
        }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        CONTROLLER_STATE_GETS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The host-provided stream remains live for this synchronous state write.
        unsafe { write_gain_state(state, self.gain.load(Ordering::Relaxed)) }
    }

    unsafe fn getParameterCount(&self) -> i32 {
        2
    }

    unsafe fn getParameterInfo(&self, index: i32, info: *mut ParameterInfo) -> tresult {
        if !(0..2).contains(&index) || info.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied one writable ParameterInfo for the validated index.
        let info = unsafe { &mut *info };
        info.id = if index == 0 {
            PARAMETER_ID
        } else {
            READ_ONLY_PARAMETER_ID
        };
        copy_utf16(if index == 0 { "Gain" } else { "Meter" }, &mut info.title);
        copy_utf16(if index == 0 { "Gain" } else { "Meter" }, &mut info.shortTitle);
        copy_utf16("", &mut info.units);
        info.stepCount = 0;
        info.defaultNormalizedValue = if index == 0 { 1.0 } else { 0.0 };
        info.unitId = 0;
        info.flags = if index == 0 {
            ParameterInfo_::ParameterFlags_::kCanAutomate
        } else {
            ParameterInfo_::ParameterFlags_::kIsReadOnly
        };
        kResultOk
    }

    unsafe fn getParamStringByValue(
        &self,
        id: ParamID,
        value: ParamValue,
        string: *mut String128,
    ) -> tresult {
        if id != PARAMETER_ID || string.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied one writable String128 for this synchronous call.
        unsafe { copy_utf16(&value.to_string(), &mut *string) };
        kResultOk
    }

    unsafe fn getParamValueByString(
        &self,
        _id: ParamID,
        _string: *mut TChar,
        _value: *mut ParamValue,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn normalizedParamToPlain(&self, id: ParamID, value: ParamValue) -> ParamValue {
        if id == PARAMETER_ID { value } else { 0.0 }
    }

    unsafe fn plainParamToNormalized(&self, id: ParamID, value: ParamValue) -> ParamValue {
        if id == PARAMETER_ID { value } else { 0.0 }
    }

    unsafe fn getParamNormalized(&self, id: ParamID) -> ParamValue {
        if id == PARAMETER_ID {
            f64::from_bits(self.gain.load(Ordering::Relaxed))
        } else {
            0.0
        }
    }

    unsafe fn setParamNormalized(&self, id: ParamID, value: ParamValue) -> tresult {
        if id != PARAMETER_ID || !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return kInvalidArgument;
        }
        self.gain.store(value.to_bits(), Ordering::Relaxed);
        kResultOk
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, _name: FIDString) -> *mut IPlugView {
        ptr::null_mut()
    }
}

struct Factory;

impl Class for Factory {
    type Interfaces = (IPluginFactory,);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied one writable PFactoryInfo.
        let info = unsafe { &mut *info };
        copy_c_string("Superi", &mut info.vendor);
        copy_c_string("https://github.com/thebriangao/Superi", &mut info.url);
        copy_c_string("fixture@superi.invalid", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as i32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        1
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if index != 0 || info.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The host supplied one writable PClassInfo for the sole class.
        let info = unsafe { &mut *info };
        info.cid = FixtureEffect::CID;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as i32;
        copy_c_string("Audio Module Class", &mut info.category);
        copy_c_string(EFFECT_NAME, &mut info.name);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        class_id: FIDString,
        interface_id: FIDString,
        object: *mut *mut c_void,
    ) -> tresult {
        if class_id.is_null() || interface_id.is_null() || object.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: VST3 supplies class_id as a readable 16-byte TUID for this call.
        if unsafe { *(class_id as *const TUID) } != FixtureEffect::CID {
            return kInvalidArgument;
        }
        let instance = ComWrapper::new(FixtureEffect::new())
            .to_com_ptr::<FUnknown>()
            .expect("fixture implements FUnknown");
        let pointer = instance.as_ptr();
        // SAFETY: The wrapper remains owned by instance for the complete queryInterface call. A
        // successful query transfers one owned interface reference into object.
        unsafe { ((*(*pointer).vtbl).queryInterface)(pointer, interface_id as *mut TUID, object) }
    }
}

fn reset_evidence() {
    EVENT_COUNT.store(0, Ordering::SeqCst);
    EVENT_SEQUENCE.store(0, Ordering::SeqCst);
    OBSERVED_SAMPLE_RATE.store(0, Ordering::SeqCst);
    OBSERVED_START_SAMPLE.store(0, Ordering::SeqCst);
    OBSERVED_FRAMES.store(0, Ordering::SeqCst);
    OBSERVED_MODE.store(-1, Ordering::SeqCst);
    OBSERVED_SAMPLE_SIZE.store(-1, Ordering::SeqCst);
    OBSERVED_CHANNELS.store(0, Ordering::SeqCst);
    PROCESS_COUNT.store(0, Ordering::SeqCst);
    HOST_OBJECTS_VERIFIED.store(false, Ordering::SeqCst);
    CALLBACK_ALLOCATIONS.store(0, Ordering::SeqCst);
    COMPONENT_STATE_GETS.store(0, Ordering::SeqCst);
    COMPONENT_STATE_SETS.store(0, Ordering::SeqCst);
    CONTROLLER_COMPONENT_STATE_SETS.store(0, Ordering::SeqCst);
    CONTROLLER_STATE_GETS.store(0, Ordering::SeqCst);
    CONTROLLER_STATE_SETS.store(0, Ordering::SeqCst);
}

fn write_evidence() {
    let Ok(path) = std::env::var("SUPERI_VST3_FIXTURE_EVIDENCE") else {
        return;
    };
    let body = format!(
        "events={}\nsequence={:X}\nsample_rate={}\nstart_sample={}\nframes={}\nmode={}\nsample_size={}\nchannels={}\nprocesses={}\nhost_objects={}\ncallback_allocations={}\ncomponent_state_gets={}\ncomponent_state_sets={}\ncontroller_component_state_sets={}\ncontroller_state_gets={}\ncontroller_state_sets={}\n",
        EVENT_COUNT.load(Ordering::SeqCst),
        EVENT_SEQUENCE.load(Ordering::SeqCst),
        f64::from_bits(OBSERVED_SAMPLE_RATE.load(Ordering::SeqCst)),
        OBSERVED_START_SAMPLE.load(Ordering::SeqCst),
        OBSERVED_FRAMES.load(Ordering::SeqCst),
        OBSERVED_MODE.load(Ordering::SeqCst),
        OBSERVED_SAMPLE_SIZE.load(Ordering::SeqCst),
        OBSERVED_CHANNELS.load(Ordering::SeqCst),
        PROCESS_COUNT.load(Ordering::SeqCst),
        u8::from(HOST_OBJECTS_VERIFIED.load(Ordering::SeqCst)),
        CALLBACK_ALLOCATIONS.load(Ordering::SeqCst),
        COMPONENT_STATE_GETS.load(Ordering::SeqCst),
        COMPONENT_STATE_SETS.load(Ordering::SeqCst),
        CONTROLLER_COMPONENT_STATE_SETS.load(Ordering::SeqCst),
        CONTROLLER_STATE_GETS.load(Ordering::SeqCst),
        CONTROLLER_STATE_SETS.load(Ordering::SeqCst),
    );
    std::fs::write(path, body).expect("write VST3 fixture evidence");
}

#[cfg(target_os = "windows")]
#[no_mangle]
extern "system" fn InitDll() -> bool {
    reset_evidence();
    record_event(1);
    true
}

#[cfg(target_os = "windows")]
#[no_mangle]
extern "system" fn ExitDll() -> bool {
    record_event(12);
    write_evidence();
    true
}

#[cfg(target_os = "macos")]
#[no_mangle]
extern "system" fn bundleEntry(_bundle: *mut c_void) -> bool {
    reset_evidence();
    record_event(1);
    true
}

#[cfg(target_os = "macos")]
#[no_mangle]
extern "system" fn bundleExit() -> bool {
    record_event(12);
    write_evidence();
    true
}

#[cfg(target_os = "linux")]
#[no_mangle]
extern "system" fn ModuleEntry(_library: *mut c_void) -> bool {
    reset_evidence();
    record_event(1);
    true
}

#[cfg(target_os = "linux")]
#[no_mangle]
extern "system" fn ModuleExit() -> bool {
    record_event(12);
    write_evidence();
    true
}

#[no_mangle]
extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory)
        .to_com_ptr::<IPluginFactory>()
        .expect("fixture factory implements IPluginFactory")
        .into_raw()
}
