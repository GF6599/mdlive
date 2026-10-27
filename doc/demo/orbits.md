---
title: Orbits
tags: [physics, demo]
---

# Orbits

A planet moves on an ellipse with the sun at one focus. Its speed changes along the way: it is fastest at the point nearest the sun. The [units page](units.md) lists the constants.

> [!NOTE]
> The orbits here leave out the pull of the planets on each other.

## Kepler's laws

| Law | Says |
|---|---|
| First | Each orbit is an ellipse, with the sun at one focus |
| Second | A line from the sun to the planet sweeps equal areas in equal times |
| Third | The square of the period grows with the cube of the semi-major axis |

The third law, for a period $T$ and a semi-major axis $a$:

$$T^2 = \frac{4\pi^2}{G M} a^3$$

## The orbit in the plane

A point on the orbit turns through the angle $\theta$ by the rotation matrix:

$$R(\theta) = \begin{bmatrix} \cos\theta & -\sin\theta \\ \sin\theta & \cos\theta \end{bmatrix}$$

## A short simulation

```python
def step(position, velocity, dt):
    acceleration = -position / abs(position) ** 3
    velocity += acceleration * dt
    position += velocity * dt
    return position, velocity
```

<details>
<summary>Why the step uses velocity first</summary>

Updating the velocity before the position keeps the energy of the orbit steady over many steps. This is the semi-implicit Euler method.

</details>

## Periods of the planets

| Planet | Semi-major axis (AU) | Period (years) |
|---|---:|---:|
| Mercury | 0.39 | 0.24 |
| Venus | 0.72 | 0.62 |
| Earth | 1.00 | 1.00 |
| Mars | 1.52 | 1.88 |
| Jupiter | 5.20 | 11.86 |
