# Official references

**Phiên bản:** 1.0.0  
**Reference snapshot:** 2026-07-23  
**Policy:** Ưu tiên primary/official sources; repository examples không được coi là normative domain standard

## 1. PostgreSQL

- PostgreSQL 18 documentation: https://www.postgresql.org/docs/18/
- Current supported documentation: https://www.postgresql.org/docs/current/
- Recursive queries: https://www.postgresql.org/docs/current/queries-with.html
- Full-text search: https://www.postgresql.org/docs/current/textsearch.html
- Indexes: https://www.postgresql.org/docs/current/indexes.html
- Backup/restore: https://www.postgresql.org/docs/current/backup.html
- Row security: https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- `pg_trgm`: https://www.postgresql.org/docs/current/pgtrgm.html

## 2. USDA FoodData Central

- Data documentation: https://fdc.nal.usda.gov/data-documentation/
- API guide: https://fdc.nal.usda.gov/api-guide/
- OpenAPI: https://fdc.nal.usda.gov/api-spec/fdc_api.html
- Downloadable datasets: https://fdc.nal.usda.gov/download-datasets/
- Foundation Foods: https://fdc.nal.usda.gov/foundation-foods-documentation/

Architectural use:

- Phân biệt data types và derivation semantics.
- Import release thay vì gọi API trên request path.
- Bảo toàn FDC ID, data type, release và raw fields.
- Không coi branded label, foundation analysis và compiled survey food là evidence tương đương.

## 3. FAO/INFOODS

- INFOODS portal: https://www.fao.org/infoods/
- Food matching guidelines: https://www.fao.org/fileadmin/templates/food_composition/documents/upload/INFOODSGuidelinesforFoodMatching_version_1_2.pdf
- Data checking guidelines: https://www.fao.org/fileadmin/templates/food_composition/documents/Guidelines_data_checking_final_oct2012.pdf
- Food energy methods: https://www.fao.org/4/y5022e/y5022e00.htm

Architectural use:

- Food matching là controlled evidence selection, không chỉ string similarity.
- Missing/trace/zero và unit/basis phải rõ.
- Energy factors phải versioned và documented.

## 4. EuroFIR

- Recipe calculation procedures: https://www.eurofir.org/report-on-collection-of-rules-on-use-of-recipe-calculation-procedures-including-the-use-of-yield-and-retention-factors-for-imputing-nutrient-values-for-composite-foods/
- Recipe/composite food explanation: https://www.eurofir.org/how-do-recipes-and-composite-foods-come-to-their-nutritional-values/

Architectural use:

- Phân biệt yield factor và retention factor.
- Ghi derivation method và calculation trace.
- Không áp dụng retention/yield mặc định khi thiếu evidence.

## 5. Food ontology/taxonomy references

- FoodOn repository: https://github.com/FoodOntology/foodon
- FoodOn project: https://foodon.org/
- OBO Foundry FoodOn: https://obofoundry.org/ontology/foodon.html
- LanguaL: https://www.langual.org/

Architectural use:

- Học facet-based description và external identifiers.
- Không import toàn ontology vào transactional schema trong MVP.
- Mapping external ontology phải versioned và optional.

## 6. Open Food Facts

- API documentation: https://openfoodfacts.github.io/openfoodfacts-server/api/
- Server repository: https://github.com/openfoodfacts/openfoodfacts-server
- Data/license terms: https://world.openfoodfacts.org/terms-of-use

Architectural use:

- Branded/package enrichment và barcode phases.
- Bắt buộc license/attribution review.
- Community data phải có verification/quality treatment.

## 7. Rust/runtime stack

- Rust: https://www.rust-lang.org/
- Tokio: https://tokio.rs/
- Axum documentation: https://docs.rs/axum/latest/axum/
- SQLx documentation: https://docs.rs/sqlx/latest/sqlx/
- Serde: https://serde.rs/
- Tower: https://docs.rs/tower/latest/tower/
- tracing: https://docs.rs/tracing/latest/tracing/

Version policy:

- Pin dependencies in `Cargo.lock`.
- Upgrade bằng dependency PR, CI, benchmark và release note.
- Không ghi patch version cứng trong architecture trừ deployment baseline.

## 8. Observability

- OpenTelemetry specification: https://opentelemetry.io/docs/specs/otel/
- OpenTelemetry Rust: https://opentelemetry.io/docs/languages/rust/
- Semantic conventions: https://opentelemetry.io/docs/specs/semconv/

## 9. Open-source/market references

- Ingredient Parser: https://github.com/strangetom/ingredient-parser
- Mealie: https://github.com/mealie-recipes/mealie
- Tandoor Recipes: https://github.com/TandoorRecipes/recipes
- NutriNutri: https://github.com/riki137/nutrinutri
- FoodYou: https://github.com/maksimowiczm/FoodYou
- NutriBench: https://github.com/DongXzz/NutriBench
- FoodOn: https://github.com/FoodOntology/foodon

Các repo này dùng để học UX, schema, parsing, evaluation và operations. Trước khi dùng code/data:

1. pin commit/tag;
2. kiểm tra license và transitive assets;
3. chạy security review;
4. viết adapter hoặc port tối thiểu;
5. không sao chép domain assumptions không phù hợp.

## 10. Research evidence policy

- Mọi thay đổi calculation/source selection phải trích nguồn trong ADR hoặc spec.
- Blog/vendor material chỉ là supplementary evidence.
- Repo README metrics không được coi là independently validated.
- Volatile data như stars/version phải ghi snapshot date hoặc tránh dùng.
- PDF có bảng/công thức cần review trực tiếp trang liên quan.
- Khi sources mâu thuẫn, lưu cả alternatives và quyết định policy.
