CREATE TABLE catalog.catalog_release_food_name (
    catalog_release_id uuid NOT NULL REFERENCES catalog.catalog_release(id),
    food_name_id uuid NOT NULL REFERENCES catalog.food_name(id),
    PRIMARY KEY (catalog_release_id, food_name_id)
);

CREATE TABLE catalog.catalog_release_profile (
    catalog_release_id uuid NOT NULL REFERENCES catalog.catalog_release(id),
    profile_id uuid NOT NULL REFERENCES composition.composition_profile(id),
    PRIMARY KEY (catalog_release_id, profile_id)
);

