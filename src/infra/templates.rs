use tera::Tera;



#[derive(Clone)]
pub struct TemplateEngine;

impl TemplateEngine {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub fn render(&self, template_str: &str, context: &tera::Context) -> std::result::Result<String, tera::Error> {
        let result = Tera::one_off(template_str, context, true)?;
        Ok(result)
    }
}
