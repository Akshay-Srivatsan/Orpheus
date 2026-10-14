fn main() {
    for d in orpheus_lib::audio::output::list_devices() {
        println!("{:?}", d);
    }
}
