use std::convert::{TryFrom, TryInto};
use std::io::{Error, ErrorKind, Read, Result, Write};
use std::mem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    None,
    Sub,
    Up,
    Avg,
    Paeth,
}

impl TryFrom<u8> for FilterType {
    type Error = ();

    fn try_from(n: u8) -> std::result::Result<FilterType, ()> {
        match n {
            0 => Ok(FilterType::None),
            1 => Ok(FilterType::Sub),
            2 => Ok(FilterType::Up),
            3 => Ok(FilterType::Avg),
            4 => Ok(FilterType::Paeth),
            _ => Err(()),
        }
    }
}

fn paeth_predict(left: u8, above: u8, upperleft: u8) -> u8 {
    let expand_left = i16::from(left);
    let expand_above = i16::from(above);
    let expand_upperleft = i16::from(upperleft);

    let initial_estimate = expand_left + expand_above - expand_upperleft;

    let dist_left = (initial_estimate - expand_left).abs();
    let dist_above = (initial_estimate - expand_above).abs();
    let dist_upperleft = (initial_estimate - expand_upperleft).abs();

    if dist_left <= dist_above && dist_left <= dist_upperleft {
        left
    } else if dist_above <= dist_upperleft {
        above
    } else {
        upperleft
    }
}

pub fn decode_row(filter: FilterType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    use self::FilterType::*;
    let len = current.len();
    let bpp = bpp.min(len);

    match filter {
        None => (),
        Sub => {
            for i in bpp..len {
                current[i] = current[i].wrapping_add(current[i - bpp]);
            }
        }
        Up => {
            for i in 0..len {
                current[i] = current[i].wrapping_add(previous[i]);
            }
        }
        Avg => {
            for i in 0..bpp {
                current[i] = current[i].wrapping_add(previous[i] / 2);
            }

            for i in bpp..len {
                // De haakjes stonden verkeerd: `a + b / 2` in plaats van
                // `(a + b) / 2`. De PNG-specificatie (RFC 2083 §6.4) deelt de
                // SOM van links en boven door twee, en `/` bond hier alleen aan
                // `previous[i]`. Elke rij met filtertype 3 kwam er daardoor
                // verkeerd uit -- geen foutmelding, gewoon andere bytes.
                current[i] = current[i].wrapping_add(
                    ((i16::from(current[i - bpp]) + i16::from(previous[i])) / 2) as u8,
                );
            }
        }
        Paeth => {
            for i in 0..bpp {
                current[i] = current[i].wrapping_add(paeth_predict(0, previous[i], 0));
            }

            for i in bpp..len {
                current[i] = current[i].wrapping_add(paeth_predict(
                    current[i - bpp],
                    previous[i],
                    previous[i - bpp],
                ));
            }
        }
    }
}

pub fn decode_frame(
    content: &[u8],
    bytes_per_pixel: usize,
    pixels_per_row: usize,
) -> Result<Vec<u8>> {
    let bytes_per_row = bytes_per_pixel * pixels_per_row;
    let mut previous = Vec::new();
    previous.try_reserve(bytes_per_row)?;
    previous.resize(bytes_per_row, 0_u8);
    let mut current = Vec::new();
    current.try_reserve(bytes_per_row)?;
    current.resize(bytes_per_row, 0_u8);
    let mut decoded = Vec::new();
    let mut pos = 0;
    while pos < content.len() {
        if let Ok(filter) = content[pos].try_into() {
            pos += 1;
            (&content[pos..]).read_exact(current.as_mut_slice())?;
            pos += bytes_per_row;

            decode_row(
                filter,
                bytes_per_pixel,
                previous.as_slice(),
                current.as_mut_slice(),
            );
            decoded.write_all(current.as_slice())?;
            mem::swap(&mut previous, &mut current);
        } else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("invalid PNG filter type ({})", content[pos]),
            ));
        }
    }
    Ok(decoded)
}

pub fn encode_row(method: FilterType, bpp: usize, previous: &[u8], current: &mut [u8]) {
    use self::FilterType::*;
    let len = current.len();
    let bpp = bpp.min(len);

    match method {
        None => (),
        Sub => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(current[i - bpp]);
            }
        }
        Up => {
            for i in 0..len {
                current[i] = current[i].wrapping_sub(previous[i]);
            }
        }
        Avg => {
            for i in (bpp..len).rev() {
                // Same nine-bit sum as the decoder: RFC 2083 §6.5 averages
                // left and above before dividing, and `wrapping_add` here
                // truncated the sum to eight bits, so any pair adding to 256
                // or more encoded to a byte the decoder could not undo. The
                // decoder was repaired first, which left the two halves
                // disagreeing; nothing caught it because no test reached
                // `encode_row` (docs/TEST_REACHABILITY.md).
                current[i] = current[i].wrapping_sub(
                    ((i16::from(current[i - bpp]) + i16::from(previous[i])) / 2) as u8,
                );
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(previous[i] / 2);
            }
        }
        Paeth => {
            for i in (bpp..len).rev() {
                current[i] = current[i].wrapping_sub(paeth_predict(
                    current[i - bpp],
                    previous[i],
                    previous[i - bpp],
                ));
            }

            for i in 0..bpp {
                current[i] = current[i].wrapping_sub(paeth_predict(0, previous[i], 0));
            }
        }
    }
}

#[cfg(test)]
mod predictor_tests {

    /// `encode_row` is the inverse of `decode_row`, so the only test worth
    /// having runs them against each other. Both had the same defect on the
    /// Average filter — RFC 2083 §6.5 computes `floor((a + b) / 2)` in nine
    /// bits, and adding two bytes first wraps whenever a + b exceeds 255.
    /// The decoder was fixed after a corpus document rendered wrong; the
    /// encoder was never reached by a test at all.
    #[test]
    fn every_filter_round_trips_including_values_that_overflow_a_byte() {
        use super::FilterType::*;
        // 200 + 200 = 400: the case that wraps if the sum is taken in u8.
        let vorige: Vec<u8> = vec![200, 200, 200, 7, 0, 255, 128, 3];
        let bron: Vec<u8> = vec![200, 201, 202, 9, 1, 254, 127, 5];

        for filter in [None, Sub, Up, Avg, Paeth] {
            for bpp in [1usize, 3] {
                let mut werk = bron.clone();
                super::encode_row(filter, bpp, &vorige, &mut werk);
                super::decode_row(filter, bpp, &vorige, &mut werk);
                assert_eq!(
                    werk, bron,
                    "{filter:?} with bpp {bpp} did not survive encode->decode"
                );
            }
        }
    }

    use super::decode_frame;

    /// De PNG-predictor zit voor vrijwel elke gecomprimeerde stroom in een PDF.
    /// Eén verkeerde optelling verschuift niet één byte maar alle bytes erna,
    /// en het resultaat is geen foutmelding maar onleesbare inhoud.
    ///
    /// De verwachte waarden hieronder zijn met de hand uitgerekend volgens de
    /// PNG-specificatie (RFC 2083 §6), niet overgenomen uit onze eigen uitvoer.

    #[test]
    fn filter_none_passes_the_row_through() {
        // filterbyte 0, dan drie bytes
        let uit = decode_frame(&[0, 10, 20, 30], 1, 3).unwrap();
        assert_eq!(uit, vec![10, 20, 30]);
    }

    #[test]
    fn filter_sub_adds_the_pixel_to_its_left() {
        // recon[0]=5, recon[1]=5+3=8, recon[2]=8+2=10
        let uit = decode_frame(&[1, 5, 3, 2], 1, 3).unwrap();
        assert_eq!(uit, vec![5, 8, 10]);
    }

    #[test]
    fn filter_up_adds_the_row_above() {
        // rij 1 met Up en een impliciete nulrij erboven -> onveranderd
        // rij 2 met Up telt er de vorige rij bij op
        let uit = decode_frame(&[2, 10, 20, 30, 2, 1, 2, 3], 1, 3).unwrap();
        assert_eq!(uit, vec![10, 20, 30, 11, 22, 33]);
    }

    /// Average rondt naar beneden af: recon = raw + floor((links + boven) / 2).
    /// Naar boven afronden geeft precies één te veel op de helft van de bytes,
    /// wat er als ruis uitziet in plaats van als een fout.
    #[test]
    fn filter_average_rounds_down() {
        // rij 1: None -> [10, 20, 30]
        // rij 2: Avg, raw [0,0,0]
        //   i=0: links=0,  boven=10 -> floor(10/2)=5
        //   i=1: links=5,  boven=20 -> floor(25/2)=12
        //   i=2: links=12, boven=30 -> floor(42/2)=21
        let uit = decode_frame(&[0, 10, 20, 30, 3, 0, 0, 0], 1, 3).unwrap();
        assert_eq!(uit, vec![10, 20, 30, 5, 12, 21]);
    }

    /// Paeth kiest de buur die het dichtst bij de schatting links+boven-linksboven
    /// ligt, met links als beslissing bij gelijkspel. Die volgorde is de plek
    /// waar implementaties uit elkaar lopen.
    #[test]
    fn filter_paeth_picks_the_nearest_neighbour() {
        // rij 1: None -> [10, 20, 30]
        // rij 2: Paeth, raw [0,0,0]
        //   i=0: links=0, boven=10, linksboven=0 -> p=10, kiest boven=10
        //   i=1: links=10, boven=20, linksboven=10 -> p=20, kiest boven=20
        //   i=2: links=20, boven=30, linksboven=20 -> p=30, kiest boven=30
        let uit = decode_frame(&[0, 10, 20, 30, 4, 0, 0, 0], 1, 3).unwrap();
        assert_eq!(uit, vec![10, 20, 30, 10, 20, 30]);
    }

    /// Bytes per pixel bepaalt hoe ver "links" terugkijkt. Bij 3 bytes per pixel
    /// verwijst Sub naar drie posities terug, niet naar één.
    #[test]
    fn bytes_per_pixel_sets_how_far_left_reaches() {
        // 2 pixels van 3 bytes: raw [1,2,3, 10,20,30]
        // recon[0..3] = [1,2,3]; recon[3] = 10+1 = 11, [4] = 20+2 = 22, [5] = 30+3 = 33
        let uit = decode_frame(&[1, 1, 2, 3, 10, 20, 30], 3, 2).unwrap();
        assert_eq!(uit, vec![1, 2, 3, 11, 22, 33]);
    }

    #[test]
    fn an_unknown_filter_byte_is_refused() {
        assert!(
            decode_frame(&[9, 1, 2, 3], 1, 3).is_err(),
            "filter 9 bestaat niet"
        );
    }

    /// Overloop hoort om te wikkelen, niet te panieken: de specificatie rekent
    /// modulo 256 en een echte stroom leunt daarop.
    #[test]
    fn addition_wraps_at_256() {
        // Sub: recon[0]=200, recon[1]=200+100 = 300 mod 256 = 44
        let uit = decode_frame(&[1, 200, 100], 1, 2).unwrap();
        assert_eq!(uit, vec![200, 44]);
    }
}
