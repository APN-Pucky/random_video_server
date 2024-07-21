use actix_files::NamedFile;
use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use glob::glob;
use rand::seq::SliceRandom;
use std::fs::read_dir;
use std::fs::read_to_string;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use walkdir::WalkDir;

use serde::Deserialize;
use structopt::StructOpt;

#[derive(Debug, Clone)]
struct Data {
    movies: Vec<Movie>,
    config: Config,
}

#[derive(Debug, Deserialize, StructOpt, Clone)]
struct Config {
    #[structopt(short, long)]
    directory: String,

    #[structopt(short, long)]
    ip_bind: String,

    #[structopt(short, long)]
    port_bind: u16,
    #[structopt(long)]
    trailer_factor: i8,
    #[structopt(long)]
    poster_factor: i8,
    #[structopt(long)]
    fanart_factor: i8,
    #[structopt(long)]
    video_factor: i8,
}

#[derive(Debug, Deserialize, StructOpt)]
struct OptConfig {
    #[structopt(short, long, help = "Kodi videos directory")]
    directory: Option<String>,

    #[structopt(
        short,
        long,
        help = "IP for bind. 127.0.0.1 for only same machine. 0.0.0.0 for global access (default: 127.0.0.1)"
    )]
    ip_bind: Option<String>,

    #[structopt(short, long, help = "Port for bind (default: 3070)")]
    port_bind: Option<u16>,

    #[structopt(
        short,
        long,
        parse(from_os_str),
        help = "Path to config file [default: $XDG_CONFIG_HOME/random_video_server/config.toml]"
    )]
    config: Option<PathBuf>,
    #[structopt(long, help = "Show trailers N-times more likely (default: 1)")]
    trailer_factor: Option<i8>,
    #[structopt(long, help = "Show posters N-times more likely (default: 1)")]
    poster_factor: Option<i8>,
    #[structopt(long, help = "Show fanart N-times more likely (default: 1)")]
    fanart_factor: Option<i8>,
    #[structopt(long, help = "Show video N-times more likely (default: 0)")]
    video_factor: Option<i8>,
}

#[derive(Debug, Clone)]
struct Movie {
    movie: PathBuf,
    trailer: Option<PathBuf>,
    poster: Option<PathBuf>,
    fanarts: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
enum PathType {
    Video,
    Trailer,
    Poster,
    Fanart,
}

fn get_random_path(config: &Config, movie: &Movie) -> Option<(PathBuf, PathType)> {
    let mut paths = vec![];

    for _ in 0..config.trailer_factor {
        if let Some(trailer) = &movie.trailer {
            paths.push((trailer.clone(), PathType::Trailer));
        }
    }
    for _ in 0..config.poster_factor {
        if let Some(poster) = &movie.poster {
            paths.push((poster.clone(), PathType::Poster));
        }
    }
    for _ in 0..config.fanart_factor {
        for fanart in &movie.fanarts {
            paths.push((fanart.clone(), PathType::Fanart));
        }
    }
    for _ in 0..config.video_factor {
        paths.push((movie.movie.clone(), PathType::Video));
    }

    let mut rng = rand::thread_rng();
    paths.choose(&mut rng).cloned()
}

fn get_folders_in_folder<P: AsRef<Path>>(folder: P) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Ok(entries) = read_dir(folder) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                }
            }
        }
    }
    folders
}

fn striped(root_dir: &String, p: PathBuf) -> Option<PathBuf> {
    p.strip_prefix(root_dir).ok().map(|p| p.to_path_buf())
}

fn load_movie_data(root_dir: &String) -> Vec<Movie> {
    let mut movies: Vec<Movie> = Vec::new();

    let folders: Vec<PathBuf> = get_folders_in_folder(root_dir);

    for f in folders {
        // Get the last directory component
        if let Some(name) = f.file_name() {
            if let Some(_dir) = striped(root_dir, f.clone()) {
                // mkv and avi do not work currently in ff/chrome
                // match all mp4, webm files in the folder usign glob and loop them
                for ext in ["mp4", "webm"].iter() {
                    // Movies
                    for gl in [
                        &format!("{}/{}.{}", f.display(), name.to_string_lossy(), ext).to_string(),
                        //&format!("{}/**/{}*.{}", f.display(), name.to_string_lossy(), ext) .to_string(),
                    ] {
                        match glob(gl) {
                            Ok(entries) => {
                                for entry in entries {
                                    if let Ok(path) = entry {
                                        // test that it does not end in -trailer
                                        if path.display().to_string().contains("-trailer") == false
                                        {
                                            if let Some(movie) = striped(root_dir, path) {
                                                let mut poster = None;
                                                let tmp = f.join(format!(
                                                    "{}{}",
                                                    name.to_string_lossy(),
                                                    "-poster.jpg"
                                                ));
                                                if tmp.exists() {
                                                    poster = striped(root_dir, tmp);
                                                }
                                                let tmp2 = f.join(format!(
                                                    "{}{}",
                                                    name.to_string_lossy(),
                                                    "-poster.png"
                                                ));
                                                if tmp2.exists() {
                                                    poster = striped(root_dir, tmp2);
                                                }

                                                let mut trailer = None;
                                                let tmp3 = f.join(format!(
                                                    "{}{}",
                                                    name.to_string_lossy(),
                                                    "-trailer.mp4"
                                                ));
                                                if tmp3.exists() {
                                                    trailer = striped(root_dir, tmp3);
                                                }

                                                let extensions = vec!["jpg", "png"];
                                                let fanarts: Vec<PathBuf> = WalkDir::new(f.clone())
                                                    .into_iter()
                                                    .filter_map(|e| e.ok())
                                                    .filter(|e| e.path().is_file())
                                                    // string contains ".actors" -> exclude
                                                    .filter(|e| {
                                                        e.path()
                                                            .display()
                                                            .to_string()
                                                            .contains(".actors")
                                                            == false
                                                    })
                                                    .filter(|e| {
                                                        e.path()
                                                            .display()
                                                            .to_string()
                                                            .contains("fanart")
                                                    })
                                                    // check path ends in extension
                                                    .filter(|e| {
                                                        for extension in &extensions {
                                                            if e.path()
                                                                .display()
                                                                .to_string()
                                                                .ends_with(extension)
                                                            {
                                                                return true;
                                                            }
                                                        }
                                                        false
                                                    })
                                                    .map(|e| {
                                                        e.path()
                                                            .strip_prefix(root_dir)
                                                            .ok()
                                                            .expect("Should be in root dir")
                                                            .to_path_buf()
                                                    })
                                                    .collect();

                                                movies.push(Movie {
                                                    movie,
                                                    poster,
                                                    trailer,
                                                    fanarts,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => eprintln!("Error: {}", e),
                        }
                    }
                }
            }
        }
    }

    movies
}

async fn grid(data: web::Data<Arc<Data>>) -> impl Responder {
    let mut rng = rand::thread_rng();
    //let image_data = data.lock().unwrap();

    //let image_data = load_image_data(root_dir, &extensions);
    //let random = image_data.choose_multiple(&mut rng, 100);
    //let movies = load_movie_data(&data.config.directory);
    let movies = &data.movies;
    let random = movies.choose_multiple(&mut rng, 50);

    let image_tags: Vec<String> = random
        .map(|m| {
                match get_random_path(&data.config,m) {
                Some((path, PathType::Poster)) | Some((path, PathType::Fanart)) => {
                // jpg png
                format!(
                        r#"<div class="brick"><a href="/movie/{}"><img src="/image/{}" style="display:block;float:left;"></img></a></div>"#,
                        m.movie.display(), path.display()
                       )
                },
                Some((path, PathType::Trailer)) => {
                if let Some(poster) = &m.poster {
                format!(r#"<a href="/movie/{}"><video autoplay muted loop poster="/image/{}"> <source src="/movie/{}" type="video/mp4"> Your browser does not support the video tag.  </video></a>"#,m.movie.display(),poster.display(),path.display())
                }
                else {
                format!(r#"<a href="/movie/{}"><video autoplay muted loop> <source src="/movie/{}" type="video/mp4"> Your browser does not support the video tag.  </video></a>"#,m.movie.display(),path.display())
                }
                },
                Some((_path, PathType::Video)) => {
                if let Some(poster) = &m.poster {
                format!(r#"<a href="/movie/{}"><video muted preload=metadata poster="/image/{}"> <source src="/movie/{}" type="video/mp4"> Your browser does not support the video tag.  </video></a>"#,m.movie.display(),poster.display(),m.movie.display())
                }
                else{
                    format!(r#"<a href="/movie/{}"><video muted preload=metadata> <source src="/movie/{}" type="video/mp4"> Your browser does not support the video tag.  </video></a>"#,m.movie.display(),m.movie.display())
                }
                },
                _ => {"".to_string()}
                }
        })
    .collect();

    let html_content = format!(
        r#"<!DOCTYPE html>
            <html>
            <head>
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Random Video Grid</title>
            <style>
            body {{
margin: 0;
padding: 0;
background-color: #f2f2f2;
}}
.row {{
display: flex;
flex-wrap: wrap;
width: 200%;
margin: 0;
padding: 0;
gap: 0;
}}
.brick {{
margin: 0;
padding: 0;
}}
img {{
width: 100%;
height: 33vh;
        object-fit: cover;
display: block;
}}
video {{
height: 33vh;
        object-fit: cover;
display: block;
}}
</style>

</head>
<body>
<div id="therow" class="row">
{}
</div>
<script>
window.addEventListener('DOMContentLoaded', function() {{
        var videos = document.querySelectorAll('video');
        videos.forEach(function(video) {{
                video.addEventListener('loadedmetadata', function() {{
                        var randomTime = Math.random() * video.duration;
                        video.currentTime = randomTime;
                        }});
                }});
        }});
document.addEventListener('DOMContentLoaded', function() {{
        window.addEventListener('scroll', function() {{
                if ((window.innerHeight *2 + window.scrollY) >= document.body.offsetHeight) {{
                fetch('/grid')
                .then(response => response.text())
                .then(data => {{
                        const parser = new DOMParser();
                        const doc = parser.parseFromString(data, 'text/html');
                        const bodyContent = doc.body.innerHTML;
                        document.getElementById("therow").innerHTML += doc.getElementById("therow").innerHTML;
                        }})
                .catch(error => {{
                        console.error('Error fetching and parsing data:', error);
                        }});
                }}
                }});
        }});

</script>
</body>
</html>"#,
        image_tags.join("\n")
    );

    HttpResponse::Ok()
        .content_type("text/html")
        .body(html_content)
}

async fn serve_image(data: web::Data<Arc<Data>>, path: web::Path<String>) -> impl Responder {
    let root_dir = &data.config.directory;
    let p = root_dir.to_owned() + &path.into_inner();
    let path = PathBuf::from(p);
    match is_within_folder(&PathBuf::from(root_dir), &path) {
        Ok(is_within) => {
            if is_within {
                NamedFile::open(path)
                    .map_err(|_| actix_web::error::ErrorNotFound("Image not found"))
            } else {
                Err(actix_web::error::ErrorNotFound("Image not found"))
            }
        }
        Err(e) => Err(actix_web::error::ErrorNotFound(e)),
    }
}

fn ensure_trailing_slash(path_str: String) -> String {
    // Check if the last character is a slash
    if !path_str.ends_with(std::path::MAIN_SEPARATOR) {
        // If not, append a slash
        let mut r = path_str.clone();
        r.push(std::path::MAIN_SEPARATOR);
        r
    } else {
        path_str
    }
}

fn is_within_folder(folder: &PathBuf, path: &PathBuf) -> Result<bool, String> {
    // append / if missing from folder

    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    let path = path.canonicalize().map_err(|e| e.to_string())?;

    Ok(path.starts_with(&folder))
}

async fn serve_movie(data: web::Data<Arc<Data>>, path: web::Path<String>) -> impl Responder {
    let root_dir = &data.config.directory;
    let p = root_dir.to_owned() + &path.into_inner();
    let file_path = PathBuf::from(p);
    match is_within_folder(&PathBuf::from(root_dir), &file_path) {
        Ok(is_within) => {
            if is_within {
                NamedFile::open(&file_path)
                    .map_err(|_| actix_web::error::ErrorNotFound("video not found"))
            } else {
                Err(actix_web::error::ErrorNotFound("Not within folder"))
            }
        }
        Err(e) => Err(actix_web::error::ErrorNotFound(e)),
    }
}

async fn tv(data: web::Data<Arc<Data>>) -> impl Responder {
    //let mut movies = load_movie_data(&data.config.directory);
    let mut rng = rand::thread_rng();
    let mut movies = data.movies.clone();
    movies.shuffle(&mut rng);

    let html_content = format!(
        r#"<!DOCTYPE html>
            <html lang="en">
            <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Random Video TV</title>
            <style>
            .video-container {{
position: absolute;
top: 50%;
left: 50%;
width: 95%;
height: 95%;
object-fit: cover;
transform: translate(-50%, -50%);
}}
</style>
</head>
<body>
<video id="videoPlayer" controls autoplay muted>
    <source type="video/mp4">
    Your browser does not support the video tag.
    </video>

    <script>
    document.addEventListener('DOMContentLoaded', function() {{
            const videoPlayer = document.getElementById('videoPlayer');
            const videoSources = ["{}"];

            function playRandomVideo() {{
            const randomIndex = Math.floor(Math.random() * videoSources.length);
            videoPlayer.src = videoSources[randomIndex];
            videoPlayer.play();
            }}

            videoPlayer.addEventListener('ended', playRandomVideo);

            // Play a random video when the page loads
            playRandomVideo();
            }});
</script>
</body>
</html>"#,
        movies
            .iter()
            .map(|m| "/movie/".to_owned()
                + &m.movie.to_string_lossy().to_string().replace("\"", "\\\""))
            .collect::<Vec<String>>()
            .join("\",\"")
    );

    HttpResponse::Ok()
        .content_type("text/html")
        .body(html_content)
}

async fn index() -> impl Responder {
    HttpResponse::Ok().content_type("text/html").body(
        r#"
                <!DOCTYPE html>
                <html lang="en">
                <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>Random Video Server</title>
                </head>
                <body>
                <div class="container">
                <h1>Choose Your View</h1>
                <a href="/grid" class="button">Grid</a>
                <a href="/tv" class="button">TV</a>
                </div>
                </body>
                </html>"#,
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Parse command line arguments
    let args = OptConfig::from_args();

    // Determine the config file path
    let default_config_path = dirs::config_dir()
        .map(|p| p.join("rp").join("config.toml"))
        .expect("Could not determine default config directory");
    let config_path = args.config.unwrap_or(default_config_path);

    // Read the configuration file
    let config_content = read_to_string(&config_path).unwrap_or(format!(""));
    let file_config: OptConfig = toml::from_str(&config_content)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    match args.directory.or(file_config.directory) {
        Some(directory) => {
            // Merge configurations with command line arguments taking precedence
            let config = Config {
                directory: ensure_trailing_slash(directory),
                ip_bind: args
                    .ip_bind
                    .or(file_config.ip_bind)
                    .unwrap_or_else(|| format!("127.0.0.1")),
                port_bind: args
                    .port_bind
                    .or(file_config.port_bind)
                    .unwrap_or_else(|| 3070),
                trailer_factor: args
                    .trailer_factor
                    .or(file_config.trailer_factor)
                    .unwrap_or_else(|| 1),
                poster_factor: args
                    .poster_factor
                    .or(file_config.poster_factor)
                    .unwrap_or_else(|| 1),
                fanart_factor: args
                    .fanart_factor
                    .or(file_config.fanart_factor)
                    .unwrap_or_else(|| 1),
                video_factor: args
                    .video_factor
                    .or(file_config.video_factor)
                    .unwrap_or_else(|| 0),
            };
            let data = Data {
                movies: load_movie_data(&config.directory),
                config: config.clone(),
            };
            let config_data = web::Data::new(Arc::new(data));
            let listen = config.ip_bind + ":" + &config.port_bind.to_string();
            println!("Listening on: http://{}", listen);

            HttpServer::new(move || {
                App::new()
                    .app_data(config_data.clone())
                    .route("/", web::get().to(index))
                    .route("/grid", web::get().to(grid))
                    .route("/tv", web::get().to(tv))
                    .route("/image/{filename:.*}", web::get().to(serve_image))
                    .route("/movie/{filename:.*}", web::get().to(serve_movie))
                //.service(fs::Files::new("/static", "./static").show_files_listing())
            })
            .bind(listen)?
            .run()
            .await
        }
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("directory not set."),
        )),
    }
}
