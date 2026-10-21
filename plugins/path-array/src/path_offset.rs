//! Offset the sampled path, not its anchors along centroid rays.
//! Open paths use the right-hand normal in vertex order; closed paths use
//! outward normals independent of winding. This is not a topology/boolean solver.
pub(super) fn offset(input: &[(f64, f64)], closed: bool, ex: f64, ey: f64) -> Vec<(f64, f64)> {
    if ex.abs() <= 0.001 && ey.abs() <= 0.001 { return input.to_vec(); }
    if !ex.is_finite() || !ey.is_finite() { return Vec::new(); }
    let mut points = Vec::new();
    for &p in input {
        if !p.0.is_finite() || !p.1.is_finite() { return Vec::new(); }
        if points.last().map_or(true, |q: &(f64, f64)| (p.0-q.0).hypot(p.1-q.1)>1e-9) {
            points.push(p);
        }
    }
    if closed && points.len()>1 && (points[0].0-points[points.len()-1].0).hypot(points[0].1-points[points.len()-1].1)<1e-9 { points.pop(); }
    if points.len()<2 { return points; }
    let n=points.len();
    let cross=|a:(f64,f64),b:(f64,f64)| a.0*b.1-a.1*b.0;
    let area:f64=(0..n).map(|i|cross(points[i],points[(i+1)%n])).sum();
    let winding=if closed && area<0.0 {-1.0} else {1.0};
    let edges=if closed {n} else {n-1};
    let normals:Vec<_>=(0..edges).map(|i| {
        let a=points[i]; let b=points[(i+1)%n];
        let (dx,dy)=(b.0-a.0,b.1-a.1); let length=dx.hypot(dy);
        (winding*dy/length,-winding*dx/length)
    }).collect();
    let distance=|v:(f64,f64)| ex*v.0*v.0+ey*v.1*v.1;
    let mut result=Vec::with_capacity(n);
    for i in 0..n {
        let a=normals[if i==0 {if closed {edges-1} else {0}} else {i-1}];
        let b=normals[i.min(edges-1)];
        let (da,db)=(distance(a),distance(b));
        let det=cross(a,b);
        let delta=if det.abs()>1e-9 {
            ((da*b.1-a.1*db)/det,(a.0*db-da*b.0)/det)
        } else {(b.0*db,b.1*db)};
        let p=points[i];
        if delta.0.hypot(delta.1)>16.0*ex.abs().max(ey.abs()) {
            result.push((p.0+a.0*da,p.1+a.1*da));
            result.push((p.0+b.0*db,p.1+b.1*db));
        } else {result.push((p.0+delta.0,p.1+delta.1));}
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rectangle_winding_xy_and_zero() {
        let original=vec![(4.5,6.5),(12.5,6.5),(12.5,24.5),(4.5,24.5)];
        assert_eq!(offset(&original,true,0.0,0.0),original);
        let expected=vec![(2.5,4.5),(14.5,4.5),(14.5,26.5),(2.5,26.5)];
        assert_eq!(offset(&original,true,2.0,2.0),expected);
        let reverse:Vec<_>=original.iter().rev().copied().collect();
        assert_eq!(offset(&reverse,true,2.0,2.0),expected.into_iter().rev().collect::<Vec<_>>());
        assert_eq!(offset(&original,true,2.0,-2.0)[0],(2.5,8.5));
    }
    #[test]
    fn open_endpoints_use_adjacent_edge_without_closing_segment() {
        let path=[(0.0,0.0),(10.0,0.0),(10.0,10.0)];
        assert_eq!(offset(&path,false,2.0,2.0),[(0.0,-2.0),(12.0,-2.0),(12.0,10.0)]);
        assert_eq!(offset(&[(0.0,0.0),(0.0,0.0),(10.0,0.0)],false,2.0,2.0),[(0.0,-2.0),(10.0,-2.0)]);
    }
}
