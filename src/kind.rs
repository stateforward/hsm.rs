// Kind System (Macro-based like C++ make_kind template)

pub type KindValue = u64;

// Core constants - matches C++ implementation
const ID_LENGTH: u8 = 8;
const ID_MASK: u64 = (1 << ID_LENGTH) - 1;
const DEPTH_MAX: usize = 64 / ID_LENGTH as usize; // 8 levels max

// Extract the base kind ID (lowest 8 bits)
pub const fn kind_id(kind: KindValue) -> KindValue {
    kind & ID_MASK
}

// Extract base IDs from a kind value (like C++ bases() function)
pub const fn kind_bases(kind: KindValue) -> [KindValue; DEPTH_MAX] {
    let mut bases = [0u64; DEPTH_MAX];
    let mut i = 1;
    while i < DEPTH_MAX {
        bases[i - 1] = (kind >> (ID_LENGTH as usize * i)) & ID_MASK;
        i += 1;
    }
    bases
}

// Get the base kind (everything except the lowest level)
pub const fn kind_base(kind: KindValue) -> KindValue {
    kind >> ID_LENGTH
}

// Core make_kind implementation - matches Go exactly
pub const fn make_kind_impl<const BASE_COUNT: usize>(
    id: u8,
    bases: [KindValue; BASE_COUNT],
) -> KindValue {
    let mut result = (id as u64) & ID_MASK; // No +1, matches Go
    let mut kind_ids = [0u64; DEPTH_MAX * DEPTH_MAX]; // Track unique IDs
    let mut index = 0;

    let mut base_idx = 0;
    while base_idx < BASE_COUNT {
        let base = bases[base_idx];

        // Extract each level from this base
        let mut level = 0;
        while level < DEPTH_MAX {
            let base_id = if level == 0 {
                base & ID_MASK
            } else {
                (base >> (ID_LENGTH as usize * level)) & ID_MASK
            };

            if base_id == 0 {
                break;
            }

            // Check if we've already seen this ID
            let mut already_exists = false;
            let mut check_idx = 0;
            while check_idx < index {
                if kind_ids[check_idx] == base_id {
                    already_exists = true;
                    break;
                }
                check_idx += 1;
            }

            if !already_exists {
                kind_ids[index] = base_id;
                index += 1;
                result |= base_id << (ID_LENGTH as usize * index);
            }

            level += 1;
        }
        base_idx += 1;
    }

    result
}

// is_kind implementation - matches Go exactly
pub const fn is_kind(kind: KindValue, target: KindValue) -> bool {
    let target_id = kind_id(target);

    // Check if kind exactly matches target first
    if kind == target_id {
        return true;
    }

    // Check each level of the kind hierarchy
    let mut i = 0;
    while i < DEPTH_MAX {
        let current_id = kind_id(kind >> (ID_LENGTH as usize * i));
        if current_id == target_id {
            return true;
        }
        if current_id == 0 {
            break; // End of encoded inheritance chain
        }
        i += 1;
    }

    false
}

// Main macro that works like C++ make_kind<id, bases...>()
#[macro_export]
macro_rules! make_kind {
    // No bases - just return id & ID_MASK (matches Go)
    ($id:expr) => {
        ($id as u64) & 255
    };

    // Single base
    ($id:expr, $base:expr) => {
        $crate::kind::make_kind_impl($id, [$base])
    };

    // Two bases
    ($id:expr, $base1:expr, $base2:expr) => {
        $crate::kind::make_kind_impl($id, [$base1, $base2])
    };

    // Three bases
    ($id:expr, $base1:expr, $base2:expr, $base3:expr) => {
        $crate::kind::make_kind_impl($id, [$base1, $base2, $base3])
    };

    // Four bases - should be enough for most cases
    ($id:expr, $base1:expr, $base2:expr, $base3:expr, $base4:expr) => {
        $crate::kind::make_kind_impl($id, [$base1, $base2, $base3, $base4])
    };
}

// Kind constants - following exact Go hierarchy with sequential IDs
// Sequential ID counter like Go version
// Null = 0, Element = 1, Namespace = 2, etc.
pub const NULL: KindValue = make_kind!(0); // id=0
pub const ELEMENT: KindValue = make_kind!(1); // id=1
pub const NAMESPACE: KindValue = make_kind!(2, ELEMENT); // id=2, base=Element
pub const VERTEX: KindValue = make_kind!(3, ELEMENT); // id=3, base=Element
pub const CONSTRAINT: KindValue = make_kind!(4, ELEMENT); // id=4, base=Element
pub const BEHAVIOR: KindValue = make_kind!(5, ELEMENT); // id=5, base=Element
pub const CONCURRENT: KindValue = make_kind!(6, BEHAVIOR); // id=6, base=Behavior
pub const STATE_MACHINE: KindValue = make_kind!(7, BEHAVIOR, NAMESPACE); // id=7, bases=Behavior,Namespace
pub const STATE: KindValue = make_kind!(8, VERTEX, NAMESPACE); // id=8, bases=Vertex,Namespace
pub const REGION: KindValue = make_kind!(9, ELEMENT); // id=9, base=Element
pub const TRANSITION: KindValue = make_kind!(10, ELEMENT); // id=10, base=Element
pub const INTERNAL: KindValue = make_kind!(11, TRANSITION); // id=11, base=Transition
pub const EXTERNAL: KindValue = make_kind!(12, TRANSITION); // id=12, base=Transition
pub const LOCAL: KindValue = make_kind!(13, TRANSITION); // id=13, base=Transition
pub const SELF: KindValue = make_kind!(14, TRANSITION); // id=14, base=Transition
pub const EVENT: KindValue = make_kind!(15, ELEMENT); // id=15, base=Element
pub const COMPLETION_EVENT: KindValue = make_kind!(16, EVENT); // id=16, base=Event
pub const ERROR_EVENT: KindValue = make_kind!(17, COMPLETION_EVENT); // id=17, base=CompletionEvent
pub const TIME_EVENT: KindValue = make_kind!(18, EVENT); // id=18, base=Event
pub const PSEUDOSTATE: KindValue = make_kind!(19, VERTEX); // id=19, base=Vertex
pub const INITIAL: KindValue = make_kind!(20, PSEUDOSTATE); // id=20, base=Pseudostate
pub const FINAL_STATE: KindValue = make_kind!(21, STATE); // id=21, base=State ← KEY FIX!
pub const CHOICE: KindValue = make_kind!(22, PSEUDOSTATE); // id=22, base=Pseudostate
pub const CUSTOM: KindValue = make_kind!(23, ELEMENT); // id=23, base=Element
pub const OPERATION: KindValue = make_kind!(24, BEHAVIOR); // id=24, base=Behavior
pub const CALL_EVENT: KindValue = make_kind!(25, EVENT); // id=25, base=Event
pub const DEEP_HISTORY: KindValue = make_kind!(26, PSEUDOSTATE); // id=26, base=Pseudostate
pub const SHALLOW_HISTORY: KindValue = make_kind!(27, PSEUDOSTATE); // id=27, base=Pseudostate

// Additional compatibility constants for examples/tests
pub const NAMED_ELEMENT: KindValue = ELEMENT; // Alias for compatibility
pub const SEQUENTIAL: KindValue = VERTEX; // Alias for compatibility  
pub const ATTRIBUTE: KindValue = ELEMENT; // Simple element
pub const MODEL: KindValue = STATE_MACHINE; // Alias for compatibility
pub const JUNCTION: KindValue = PSEUDOSTATE; // Alias for compatibility
