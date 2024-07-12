use loco_rs::{boot::run_task, task, testing};
use moonlit_binge::app::App;
use serial_test::serial;

#[tokio::test]
async fn test_can_seed_data() {
    crate::testing::boot_with_testcontainers::<App, _, _>(|boot| async move {
        assert!(run_task::<App>(
            &boot.app_context,
            Some(&"seed_data".to_string()),
            &task::Vars::default()
        )
        .await
        .is_ok());
    })
    .await;
}
