use crate::{
    asset_paths,
    local_input::{key_code_to_string, AlgebraicNotationInputEvent, LocalPlayerInput},
    menu::Changeable,
    sounds::SoundEvent,
    uchess::{ChessState, ChessVariant, PlayOptions, PlayerActive, PlayerBundle, PlayerTeam},
    GameState,
};
use bevy::prelude::*;
use bevy_mod_reqwest::*;
use serde::Deserialize;
use strum::{EnumIter, IntoEnumIterator};
use url_builder::URLBuilder;

#[cfg(feature = "web")]
static PROTOCOL: &str = "https";
#[cfg(feature = "web")]
static IP: &str = "sarda.dev";
#[cfg(feature = "web")]
static PORT: u16 = 443;

#[cfg(not(feature = "web"))]
static PROTOCOL: &str = "http";
#[cfg(not(feature = "web"))]
static IP: &str = "127.0.0.1";
#[cfg(not(feature = "web"))]
static PORT: u16 = 8543;

fn get_host_server_url() -> URLBuilder {
    let mut url = URLBuilder::new();
    url.set_protocol(PROTOCOL);
    url.set_host(IP);
    url.set_port(PORT);
    url.add_route("uc2024");
    url
}

fn create_host_server_url(player_key: &str, chess_variant: ChessVariant) -> String {
    let mut url = get_host_server_url();
    url.add_route("create");
    url.add_param("player_key", player_key);
    url.add_param("chess_variant", &chess_variant.to_string());

    url.build()
}

fn query_game_status_url(game_key: &str, player_key: &str) -> String {
    let mut url = get_host_server_url();
    url.add_route("game");
    url.add_route(game_key);
    url.add_param("player_key", player_key);

    url.build()
}

fn join_game_status_url(game_key: &str, player_key: &str) -> String {
    let mut url = get_host_server_url();
    url.add_route("join");
    url.add_route(game_key);
    url.add_param("player_key", player_key);

    url.build()
}

fn send_move_url(game_key: &str, player_key: &str, mov: &str) -> String {
    let mut url = get_host_server_url();
    url.add_route("move");
    url.add_route(game_key);
    url.add_param("player_key", player_key);
    url.add_param("move", mov);

    url.build()
}

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<MultiplayerState>();
        app.add_event::<ErrorEvent>();

        app.add_systems(Startup, setup);

        app.add_systems(OnEnter(GameState::Multiplayer), multiplayer_setup);

        app.add_systems(OnExit(GameState::Multiplayer), teardown_music);

        app.add_systems(
            OnExit(GameState::Multiplayer),
            teardown.run_if(not(in_state(MultiplayerState::Playing))),
        );
        app.add_systems(
            OnExit(GameState::Playing),
            teardown.run_if(in_state(MultiplayerState::Playing)),
        );

        app.add_systems(
            Update,
            (query_game_status_request, query_game_status_response)
                .run_if(resource_exists::<MultiplayerGameSession>()),
        );

        app.add_systems(OnEnter(MultiplayerState::Menu), setup_menu);
        app.add_systems(OnExit(MultiplayerState::Menu), teardown_menu);

        app.add_systems(
            Update,
            process_multiplayer_menu.run_if(in_state(MultiplayerState::Menu)),
        );

        app.add_systems(OnEnter(MultiplayerState::HostMenu), setup_host_menu);
        app.add_systems(
            Update,
            process_host_menu_input.run_if(in_state(MultiplayerState::HostMenu)),
        );
        app.add_systems(OnExit(MultiplayerState::HostMenu), teardown_host_menu);

        app.add_systems(OnEnter(MultiplayerState::HostSetup), setup_host);
        app.add_systems(
            Update,
            (handle_responses_host).run_if(in_state(MultiplayerState::HostSetup)),
        );

        app.add_systems(
            Update,
            (host_waiting_response, host_waiting_input)
                .run_if(in_state(MultiplayerState::HostWaiting)),
        );

        app.add_systems(OnEnter(MultiplayerState::JoinInput), setup_join_input);
        app.add_systems(
            Update,
            process_join_input.run_if(in_state(MultiplayerState::JoinInput)),
        );

        app.add_systems(OnEnter(MultiplayerState::Join), setup_join);
        app.add_systems(
            Update,
            join_waiting_response.run_if(in_state(MultiplayerState::Join)),
        );

        app.add_systems(
            OnEnter(MultiplayerState::Playing),
            setup_multiplayer_playing,
        );

        app.add_systems(
            First,
            (online_player_input, send_local_player_move)
                .run_if(in_state(MultiplayerState::Playing).and_then(in_state(GameState::Playing))),
        );

        app.add_systems(OnExit(MultiplayerState::Error), teardown_error);
        app.add_systems(
            Update,
            (
                error_screen_input.run_if(in_state(MultiplayerState::Error)),
                handle_multiplayer_error,
            ),
        );
    }
}

#[derive(Debug, Default, Component)]
pub struct PlayerKey {
    pub key: String,
}

impl PlayerKey {
    pub fn new() -> Self {
        Self {
            key: uuid::Uuid::new_v4().to_string(),
        }
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((PlayerKey::new(),));
}

#[derive(Resource)]
struct QueryTimer(pub Timer);

#[derive(Debug, Component)]
struct MultiplayerMusic;

fn multiplayer_setup(
    mut commands: Commands,
    mut menu_state: ResMut<NextState<MultiplayerState>>,
    asset_server: Res<AssetServer>,
) {
    menu_state.set(MultiplayerState::Menu);

    commands.insert_resource(QueryTimer(Timer::from_seconds(0.5, TimerMode::Repeating)));

    commands.spawn((
        AudioBundle {
            source: asset_server.load(asset_paths::music::MULTIPLAYER_MENU),
            settings: PlaybackSettings::LOOP,
            ..default()
        },
        MultiplayerMusic,
    ));
}

fn teardown_music(mut commands: Commands, music: Query<Entity, With<MultiplayerMusic>>) {
    for e in music.iter() {
        commands.entity(e).despawn_recursive();
    }
}

fn teardown(
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    host: Query<Entity, With<Host>>,
) {
    debug!("Running teardown");
    multiplayer_state.set(MultiplayerState::None);

    commands.remove_resource::<QueryTimer>();
    commands.remove_resource::<MultiplayerGameSession>();

    for e in host.iter() {
        commands.entity(e).despawn_recursive();
    }
}

#[derive(Debug, Default, EnumIter, Component, PartialEq, Eq, Hash, Copy, Clone)]
pub enum MultiplayerOptions {
    #[default]
    Host,
    Join,
    Back,
}

impl MultiplayerOptions {
    fn change(self, delta: i32) -> Self {
        let options = MultiplayerOptions::iter().collect::<Vec<_>>();
        let index = options
            .iter()
            .position(|&option| option == self)
            .unwrap_or_default();

        let new_index = (index as i32 + delta).rem_euclid(options.len() as i32) as usize;

        options[new_index]
    }
}

impl ToString for MultiplayerOptions {
    fn to_string(&self) -> String {
        match self {
            MultiplayerOptions::Host => "Host",
            MultiplayerOptions::Join => "Join",
            MultiplayerOptions::Back => "Back",
        }
        .to_string()
    }
}

#[derive(Debug, Default, Component)]
pub struct MultiplayerMenuInput {
    pub selected: MultiplayerOptions,
}

fn setup_menu(mut commands: Commands) {
    commands.spawn((MultiplayerMenuInput::default(),));
}

fn teardown_menu(mut commands: Commands, query: Query<Entity, With<MultiplayerMenuInput>>) {
    for e in query.iter() {
        commands.entity(e).despawn_recursive();
    }
}

fn process_multiplayer_menu(
    mut menu_input: Query<&mut MultiplayerMenuInput>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut sound_events: EventWriter<SoundEvent>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    let mut input = menu_input.single_mut();

    if keyboard_input.just_pressed(KeyCode::Up) {
        input.selected = input.selected.change(-1);
        sound_events.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Down) {
        input.selected = input.selected.change(1);
        sound_events.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Return) {
        match input.selected {
            MultiplayerOptions::Host => multiplayer_state.set(MultiplayerState::HostMenu),
            MultiplayerOptions::Join => multiplayer_state.set(MultiplayerState::JoinInput),
            MultiplayerOptions::Back => game_state.set(GameState::Menu),
        }

        sound_events.send(SoundEvent::Select);
    }

    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
    }
}

#[derive(Debug, Default, States, Hash, PartialEq, Eq, Clone, Copy)]
pub enum MultiplayerState {
    #[default]
    None,
    Menu,
    HostMenu,
    HostSetup,
    HostWaiting,
    JoinInput,
    Join,
    Playing,
    Error,
}

#[derive(Debug, EnumIter, PartialEq, Eq, Hash, Copy, Clone, Default)]
pub enum HostMenuOptions {
    #[default]
    ChessVariant,
    Start,
    Back,
}

impl Changeable for HostMenuOptions {}

#[derive(Debug, Component, Default)]
pub struct HostMenu {
    pub chess_variant: ChessVariant,
    pub selected: HostMenuOptions,
}

fn setup_host_menu(mut commands: Commands, despawn_query: Query<Entity, With<HostMenu>>) {
    for e in despawn_query.iter() {
        commands.entity(e).despawn_recursive();
    }
    commands.spawn((HostMenu::default(),));
}

fn teardown_host_menu() {}

fn process_host_menu_input(
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    keyboard_input: Res<Input<KeyCode>>,
    mut sound_events: EventWriter<SoundEvent>,
    mut menu_input: Query<&mut HostMenu>,
) {
    let mut menu_input = menu_input.single_mut();

    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
        multiplayer_state.set(MultiplayerState::None);
        sound_events.send(SoundEvent::Select);
    }

    if keyboard_input.just_pressed(KeyCode::Return) {
        match menu_input.selected {
            HostMenuOptions::Start | HostMenuOptions::ChessVariant => {
                multiplayer_state.set(MultiplayerState::HostSetup);
            }
            HostMenuOptions::Back => {
                multiplayer_state.set(MultiplayerState::Menu);
            }
        }
        sound_events.send(SoundEvent::Select);
    }

    if keyboard_input.just_pressed(KeyCode::Up) {
        menu_input.selected = menu_input.selected.change(-1);
        sound_events.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Down) {
        menu_input.selected = menu_input.selected.change(1);
        sound_events.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Left) {
        menu_input.chess_variant = menu_input.chess_variant.change(-1);
        sound_events.send(SoundEvent::MoveMenu);
    }

    if keyboard_input.just_pressed(KeyCode::Right) {
        menu_input.chess_variant = menu_input.chess_variant.change(1);
        sound_events.send(SoundEvent::MoveMenu);
    }
}

#[derive(Debug, Component, Default)]
pub struct Host;

#[derive(Debug, Component)]
struct CreateGameResponse;

fn setup_host(
    mut commands: Commands,
    player_key: Query<&PlayerKey>,
    menu_options: Query<&HostMenu>,
) {
    commands.spawn(Host::default());

    let player_key = &player_key.single().key;
    let chess_variant = menu_options.single().chess_variant;

    if let Ok(url) = create_host_server_url(player_key, chess_variant)
        .as_str()
        .try_into()
    {
        let req = ReqwestRequest::new(reqwest::Request::new(reqwest::Method::POST, url));
        commands.spawn((req, CreateGameResponse));
    }
}

#[derive(Debug, Deserialize)]
struct GameCreatedResponse {
    game_key: String,
}

#[derive(Debug, Resource)]
pub struct MultiplayerGameSession {
    pub game_key: String,
    host: Option<PlayerTeam>,
    moves: Vec<String>,
}

fn handle_responses_host(
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut error_writer: EventWriter<ErrorEvent>,
    results: Query<(Entity, &ReqwestBytesResult), With<CreateGameResponse>>,
) {
    for (e, res) in results.iter() {
        let response = match res.deserialize_json::<GameCreatedResponse>() {
            Some(res) => res,
            None => {
                error_writer.send(ErrorEvent {
                    message: "Failed to deserialize game created response".to_string(),
                });
                return;
            }
        };
        multiplayer_state.set(MultiplayerState::HostWaiting);

        commands.insert_resource(MultiplayerGameSession {
            game_key: response.game_key,
            host: None,
            moves: Vec::new(),
        });

        // Done with this entity
        commands.entity(e).despawn_recursive();
    }
}

fn host_waiting_input(
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut sound_events: EventWriter<SoundEvent>,
    keyboard_input: Res<Input<KeyCode>>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Menu);
        multiplayer_state.set(MultiplayerState::None);
        sound_events.send(SoundEvent::Select);
    }
}

#[derive(Debug, Component)]
pub struct StatusQuery;

fn query_game_status_request(
    mut commands: Commands,
    mut query_timer: ResMut<QueryTimer>,
    multiplayer_session: Res<MultiplayerGameSession>,
    time: Res<Time>,
    player_key: Query<&PlayerKey>,
) {
    if query_timer.0.tick(time.delta()).just_finished() {
        let player_key = &player_key.single().key;

        let url = query_game_status_url(&multiplayer_session.game_key, player_key)
            .as_str()
            .try_into()
            .unwrap();
        let req = ReqwestRequest::new(reqwest::Request::new(reqwest::Method::GET, url));
        commands.spawn((req, StatusQuery));

        query_timer.0.reset();
    }
}

#[derive(Debug, Deserialize)]
pub struct GameQueryResponse {
    pub moves: Vec<String>,
    pub game_ready: bool,
    pub host_team: PlayerTeam,
    pub game_complete: bool,
}

fn query_game_status_response(
    mut commands: Commands,
    mut multiplayer_session: ResMut<MultiplayerGameSession>,
    results: Query<(Entity, &ReqwestBytesResult), With<StatusQuery>>,
) {
    for (e, res) in results.iter() {
        let response = match res.deserialize_json::<GameQueryResponse>() {
            Some(res) => res,
            None => {
                error!("Failed to deserialize game query response");
                continue;
            }
        };
        if response.game_ready {
            multiplayer_session.host = Some(response.host_team);
            multiplayer_session.moves = response.moves;
        }

        // Done with this entity
        commands.entity(e).despawn_recursive();
    }
}

fn host_waiting_response(
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    multiplayer_session: Res<MultiplayerGameSession>,
    menu_input: Query<&HostMenu>,
) {
    if multiplayer_session.host.is_some() {
        let chess_variant = menu_input.single().chess_variant;
        commands.insert_resource(PlayOptions { chess_variant });

        multiplayer_state.set(MultiplayerState::Playing);
    }
}

#[derive(Debug, Default, Resource)]
pub struct JoinInput {
    pub game_key: String,
}

fn setup_join_input(mut commands: Commands) {
    commands.insert_resource(JoinInput::default());
}

fn process_join_input(
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut join_input: ResMut<JoinInput>,
    keyboard_input: Res<Input<KeyCode>>,
    mut sound_events: EventWriter<SoundEvent>,
) {
    keyboard_input.get_just_pressed().for_each(|key| {
        if *key == KeyCode::Escape {
            multiplayer_state.set(MultiplayerState::Menu);
        } else if *key == KeyCode::Return {
            multiplayer_state.set(MultiplayerState::Join);
        } else if *key == KeyCode::Back {
            join_input.game_key.pop();
            sound_events.send(SoundEvent::Backspace);
        } else {
            if join_input.game_key.len() >= 6 {
                sound_events.send(SoundEvent::Error);
                return;
            }
            join_input.game_key += key_code_to_string(*key);
            sound_events.send(SoundEvent::KeyInput);
        }
    });
}

#[derive(Debug, Component)]
struct JoinResponse;

fn setup_join(mut commands: Commands, join_input: Res<JoinInput>, player_key: Query<&PlayerKey>) {
    let player_key = &player_key.single().key;

    let url = join_game_status_url(&join_input.game_key, player_key)
        .as_str()
        .try_into()
        .unwrap();
    let req = ReqwestRequest::new(reqwest::Request::new(reqwest::Method::POST, url));
    commands.spawn((req, JoinResponse));

    commands.remove_resource::<JoinInput>();
}

#[derive(Debug, Deserialize)]
pub struct StandardResponse {
    pub success: String,
}

#[derive(Debug, Deserialize)]
pub struct JoinResponseBody {
    pub game_key: String,
    pub host: PlayerTeam,
    pub chess_variant: ChessVariant,
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

fn join_waiting_response(
    mut commands: Commands,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    mut error_writer: EventWriter<ErrorEvent>,
    results: Query<(Entity, &ReqwestBytesResult), With<JoinResponse>>,
) {
    for (e, res) in results.iter() {
        let response = match res.deserialize_json::<JoinResponseBody>() {
            Some(res) => res,
            None => {
                error!(
                    "Failed to deserialize join response {}",
                    res.as_str().unwrap_or_default()
                );
                let message = match res.deserialize_json::<ErrorResponse>() {
                    Some(response) => response.error,
                    None => "unable to connect to server".to_string(),
                };

                error_writer.send(ErrorEvent { message });

                return;
            }
        };
        // Done with this entity
        commands.entity(e).despawn_recursive();

        commands.insert_resource(PlayOptions {
            chess_variant: response.chess_variant,
        });

        commands.insert_resource(MultiplayerGameSession {
            game_key: response.game_key,
            host: Some(response.host),
            moves: Vec::default(),
        });
        multiplayer_state.set(MultiplayerState::Playing);
    }
}

fn setup_multiplayer_playing(
    mut commands: Commands,
    mut game_state: ResMut<NextState<GameState>>,
    host: Query<Entity, With<Host>>,
    multiplayer_session: Res<MultiplayerGameSession>,
) {
    let host_team = multiplayer_session.host.unwrap();

    if host.iter().len() == 1 {
        commands
            .spawn(PlayerBundle {
                team: host_team,
                ..default()
            })
            .insert(LocalPlayerInput);
        commands
            .spawn(PlayerBundle {
                team: host_team.other(),
                ..default()
            })
            .insert(MultiPlayerInput);
    } else if host.iter().len() == 0 {
        commands
            .spawn(PlayerBundle {
                team: host_team.other(),
                ..default()
            })
            .insert(LocalPlayerInput);

        commands
            .spawn(PlayerBundle {
                team: host_team,
                ..default()
            })
            .insert(MultiPlayerInput);
    } else {
        panic!("More than one host entity");
    }

    game_state.set(GameState::Playing);
}

#[derive(Debug, Component)]
pub struct MultiPlayerInput;

fn online_player_input(
    chess_state: Res<ChessState>,
    player_inputs: Query<&PlayerTeam, (With<PlayerActive>, With<MultiPlayerInput>)>,
    multiplayer_session: Res<MultiplayerGameSession>,
    mut an_input_writer: EventWriter<AlgebraicNotationInputEvent>,
) {
    let team = match player_inputs.get_single() {
        Ok(value) => value,
        Err(_) => return,
    };

    debug!(
        "Checking online player input {}",
        chess_state.half_move_count()
    );

    // current team has remote input
    if multiplayer_session.moves.len() > chess_state.half_move_count() as usize {
        let last_move = multiplayer_session.moves.last().unwrap();

        an_input_writer.send(AlgebraicNotationInputEvent {
            algebraic_notation: last_move.to_string(),
            team: *team,
        });
    }
}

#[derive(Debug, Component)]
struct MoveRequestResponse;

fn send_local_player_move(
    mut commands: Commands,
    mut an_input_reader: EventReader<AlgebraicNotationInputEvent>,
    multiplayer_session: Res<MultiplayerGameSession>,
    player_key: Query<&PlayerKey>,
    player_inputs: Query<&PlayerTeam, With<LocalPlayerInput>>,
) {
    let player_key = &player_key.single().key;

    let local_player_team = player_inputs.single();

    for event in an_input_reader.read() {
        if *local_player_team != event.team {
            continue;
        }

        let url = send_move_url(
            &multiplayer_session.game_key,
            player_key,
            &event.algebraic_notation,
        )
        .as_str()
        .try_into()
        .unwrap();

        let req = reqwest::Request::new(reqwest::Method::POST, url);
        let req = ReqwestRequest::new(req);
        commands.spawn((req, MoveRequestResponse));
    }

    an_input_reader.clear();
}

#[derive(Debug, Component)]
pub struct ErrorMessage {
    pub message: String,
}

impl ErrorMessage {
    fn new(message: String) -> Self {
        Self { message }
    }
}

#[derive(Debug, Event)]
struct ErrorEvent {
    message: String,
}

fn teardown_error(mut commands: Commands, error: Query<Entity, With<ErrorMessage>>) {
    for e in error.iter() {
        commands.entity(e).despawn_recursive();
    }
}

fn error_screen_input(
    mut game_state: ResMut<NextState<GameState>>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
    keyboard_input: Res<Input<KeyCode>>,
    mut sound_events: EventWriter<SoundEvent>,
) {
    if keyboard_input.any_just_pressed(
        [KeyCode::Escape, KeyCode::Return, KeyCode::A]
            .iter()
            .copied(),
    ) {
        game_state.set(GameState::Menu);
        multiplayer_state.set(MultiplayerState::None);
        sound_events.send(SoundEvent::Select);
    }
}

fn handle_multiplayer_error(
    mut commands: Commands,
    mut error_event_writer: EventReader<ErrorEvent>,
    mut multiplayer_state: ResMut<NextState<MultiplayerState>>,
) {
    for event in error_event_writer.read() {
        commands.spawn((ErrorMessage::new(event.message.clone()),));
        multiplayer_state.set(MultiplayerState::Error);
    }

    error_event_writer.clear();
}
