fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        // .type_attribute(
        //     "SubscriptionCommandInitial",
        //     "#[derive(validator::Validate)]",
        // )
        // .field_attribute(
        //     "SubscriptionCommandInitial.subscriptions",
        //     "#[validate(custom = \"crate::app::util::validator::validate_custom_length_vec\")]",
        // )
        .compile(&["proto/test_message.proto"], &["proto"])?;

    Ok(())
}
