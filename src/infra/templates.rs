use tera::Tera;



#[derive(Clone)]
pub struct TemplateEngine;

impl TemplateEngine {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub fn render(&self, template_str: &str, context: &tera::Context) -> std::result::Result<String, tera::Error> {
        // Use one_off for stateless rendering of dynamic templates
        // autoescape is set to true by default in one_off if not specified, 
        // but here we pass true explicitly to be safe.
        let result = Tera::one_off(template_str, context, true)?;
        Ok(result)
    }
}
