use specs::VecStorage;

pub struct StarMass(u32);

#[derive(Component, Debug)]
#[storage(VecStorage)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

#[derive(Component, Debug)]
#[storage(VecStorage)]
pub struct Mass {
    pub value: u32,
}

use specs::{Join, ReadStorage, System, WriteStorage};

struct Emmission;

impl<'a> System<'a> for Emmission {
    type SystemData = ReadStorage<'a, Position>;

    fn run(&mut self, position: Self::SystemData) {
        for position in position.join() {
            println!("at position ({}, {})", position.x, position.y);
        }
    }
}

struct Movement;

impl<'a> System<'a> for Movement {
    type SystemData = (ReadStorage<'a, Mass>, WriteStorage<'a, Position>);

    fn run(&mut self, (velocity, mut position): Self::SystemData) {
        for (velocity, position) in (&velocity, &mut position).join() {
            println!("at position ({}, {})", position.x, position.y);
            println!("with velocity {}", velocity.value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specs::{Builder, DispatcherBuilder, RunNow, World};

    #[test]
    fn test_world() {
        let mut world = World::new();
        world.register::<Mass>();
        world.register::<Position>();
        world
            .create_entity()
            .with(Position { x: 50, y: 50 })
            .with(Mass { value: 100 })
            .build();
        let mut dispatcher = DispatcherBuilder::new()
            .with(Emmission, "emmission", &[])
            .with(Movement, "movement", &["emmission"])
            .with(Emmission, "emmission_secondary", &["emmission"])
            .build();
        dispatcher.dispatch(&mut world.res);
        world.maintain();
    }
    #[test]
    fn test_resource_modification() {
        let mut world = World::new();
        world.add_resource(StarMass(1000));
        let mut delta = world.write_resource::<StarMass>();
        *delta = StarMass(500);
    }
    #[test]
    fn test_system() {
        let mut world = World::new();
        world.register::<Mass>();
        world.register::<Position>();
        let mut emmission = Emmission;
        emmission.run_now(&world.res);
        world.maintain();
    }
}
