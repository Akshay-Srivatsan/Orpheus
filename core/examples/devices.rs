fn main() {
    for d in orpheus_core::audio::output::list_devices() {
        println!("{:?}", d);
    }
}
