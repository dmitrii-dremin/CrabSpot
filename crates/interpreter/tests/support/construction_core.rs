//! Test-only bootstrap metadata with an executable empty Object constructor.
pub fn core(name: &str, parent: Option<&str>, interfaces: &[&str], interface: bool) -> Vec<u8> {
    let mut names = vec![name];
    names.extend(parent);
    names.extend_from_slice(interfaces);
    let mut bytes = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65];
    bytes.extend_from_slice(
        &(1 + names.len() as u16 * 2 + if parent.is_none() { 3 } else { 0 }).to_be_bytes(),
    );
    for (index, name) in names.into_iter().enumerate() {
        bytes.push(1);
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(7);
        bytes.extend_from_slice(&(1 + index as u16 * 2).to_be_bytes());
    }
    if parent.is_none() {
        for text in ["<init>", "()V", "Code"] {
            bytes.push(1);
            bytes.extend_from_slice(&(text.len() as u16).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
    }
    bytes.extend_from_slice(&(if interface { 0x601u16 } else { 0x21u16 }).to_be_bytes());
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&(if parent.is_some() { 4u16 } else { 0u16 }).to_be_bytes());
    bytes.extend_from_slice(&(interfaces.len() as u16).to_be_bytes());
    for index in 0..interfaces.len() {
        let slot = 1 + usize::from(parent.is_some()) + index;
        bytes.extend_from_slice(&(2 + slot as u16 * 2).to_be_bytes());
    }
    if parent.is_none() {
        // Original Object constructor Code: return, with initialized local0.
        bytes.extend_from_slice(&[0, 0, 0, 1, 0, 1, 0, 3, 0, 4, 0, 1, 0, 5]);
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1, 0xb1, 0, 0, 0, 0, 0, 0]);
    } else {
        bytes.extend_from_slice(&[0; 6]);
    }
    bytes
}
