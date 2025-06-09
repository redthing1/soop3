// security headers middleware

use axum::{
    extract::Request,
    http::Response,
    middleware::Next,
    body::Body,
};

/// add security headers to all responses
pub async fn add_security_headers(
    request: Request,
    next: Next,
) -> Response<Body> {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    
    // prevent framing to avoid clickjacking
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    
    // prevent mime type sniffing
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    
    // enable xss protection
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    
    // referrer policy
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    
    // content security policy for basic protection
    headers.insert(
        "Content-Security-Policy", 
        "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; object-src 'none'".parse().unwrap()
    );
    
    response
}