use crate::log;
use crate::{Body, Endpoint, Request, Response, Result, StatusCode};

use async_std::path::PathBuf as AsyncPathBuf;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'a + Send>>;
pub struct ServeDir {
    prefix: String,
    dir: PathBuf,
}

impl ServeDir {
    /// Create a new instance of `ServeDir`.
    pub(crate) fn new(prefix: String, dir: PathBuf) -> Self {
        Self { prefix, dir }
    }
}

impl<State> Endpoint<State> for ServeDir {
    fn call<'a>(&'a self, req: Request<State>) -> BoxFuture<'a, Result> {
        let path = req.url().path();
        let path = path.trim_start_matches(&self.prefix);
        let path = path.trim_start_matches('/');
        let mut file_path = self.dir.clone();
        for p in Path::new(path) {
            if p == OsStr::new(".") {
                continue;
            } else if p == OsStr::new("..") {
                file_path.pop();
            } else {
                file_path.push(&p);
            }
        }

        log::info!("Requested file: {:?}", file_path);

        let file_path = AsyncPathBuf::from(file_path);
        Box::pin(async move {
            if !file_path.starts_with(&self.dir) {
                log::warn!("Unauthorized attempt to read: {:?}", file_path);
                return Ok(Response::new(StatusCode::Forbidden));
            }
            if !file_path.exists().await {
                log::warn!("File not found: {:?}", file_path);
                return Ok(Response::new(StatusCode::NotFound));
            }
            let body = Body::from_file(&file_path).await?;
            let mut res = Response::new(StatusCode::Ok);
            res.set_body(body);
            Ok(res)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use async_std::sync::Arc;

    use std::{
        fs::{self, File},
        io::Write,
    };

    fn serve_dir(tempdir: &tempfile::TempDir) -> crate::Result<ServeDir> {
        let static_dir = tempdir.path().join("static");
        fs::create_dir(&static_dir)?;

        let file_path = static_dir.join("foo");
        let mut file = File::create(&file_path)?;
        write!(file, "Foobar")?;

        Ok(ServeDir {
            prefix: "/static/".to_string(),
            dir: static_dir,
        })
    }

    fn request(path: &str) -> crate::Request<()> {
        let request = crate::http::Request::get(
            crate::http::Url::parse(&format!("http://localhost/{}", path)).unwrap(),
        );
        crate::Request::new(Arc::new(()), request, vec![])
    }

    #[async_std::test]
    async fn ok() {
        let tempdir = tempfile::tempdir().unwrap();
        let serve_dir = serve_dir(&tempdir).unwrap();

        let req = request("static/foo");

        let resp = serve_dir.call(req).await.unwrap();
        let mut resp: crate::http::Response = resp.into();

        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body_string().await.unwrap(), "Foobar");
    }

    #[async_std::test]
    async fn not_found() {
        let tempdir = tempfile::tempdir().unwrap();
        let serve_dir = serve_dir(&tempdir).unwrap();

        let req = request("static/bar");

        let resp = serve_dir.call(req).await.unwrap();
        let resp: crate::http::Response = resp.into();

        assert_eq!(resp.status(), 404);
    }
}
