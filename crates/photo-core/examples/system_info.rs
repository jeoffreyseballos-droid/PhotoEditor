use photo_contracts::ResourceProvider;
fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&photo_core::resources::LocalResources.snapshot()).unwrap()
    );
}
